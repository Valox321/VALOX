#[cfg(feature = "cuda")]
use std::{
    env,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

#[cfg(feature = "cuda")]
use bullet::{
    game::inputs::{ChessBucketsMirrored, get_num_buckets},
    nn::{InitSettings, Shape, optimiser::AdamW},
    trainer::{
        save::SavedFormat,
        schedule::{TrainingSchedule, TrainingSteps, lr, wdl},
        settings::LocalSettings,
    },
    value::{ValueTrainerBuilder, loader::DirectSequentialDataLoader},
};

#[cfg(feature = "cuda")]
const HIDDEN: usize = 512;
#[cfg(feature = "cuda")]
const BASE_INPUTS: usize = 768;
#[cfg(feature = "cuda")]
const BUCKET_LAYOUT: [usize; 32] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 3, 3,
];
#[cfg(feature = "cuda")]
const NUM_INPUT_BUCKETS: usize = get_num_buckets(&BUCKET_LAYOUT);
#[cfg(feature = "cuda")]
const NUM_INPUTS: usize = BASE_INPUTS * NUM_INPUT_BUCKETS;
#[cfg(feature = "cuda")]
const EVAL_SCALE: f32 = 400.0;
#[cfg(feature = "cuda")]
const QA: i16 = 255;
#[cfg(feature = "cuda")]
const QB: i16 = 64;
#[cfg(feature = "cuda")]
const SUPERBATCHES: usize = 40;
#[cfg(feature = "cuda")]
const BATCH_SIZE: usize = 4096;

fn main() {
    #[cfg(not(feature = "cuda"))]
    {
        eprintln!("Valox NNUE training requires CUDA on this machine.");
        eprintln!("Set CUDA_PATH to the CUDA Toolkit folder and run with --features cuda.");
        std::process::exit(2);
    }

    #[cfg(feature = "cuda")]
    train();
}

#[cfg(feature = "cuda")]
fn train() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args.len() < 2 || args.len() > 4 {
        eprintln!(
            "Usage: cargo run --release --features cuda -- <training.data> <validation.data> [checkpoint-folder] [wdl-mix]"
        );
        std::process::exit(2);
    }

    let data_path = &args[0];
    let validation_path = &args[1];
    for path in [data_path, validation_path] {
        if !Path::new(path).is_file() {
            eprintln!("Training data file not found: {path}");
            std::process::exit(2);
        }
    }

    let training_bin = Path::new(data_path).with_extension("bin");
    let validation_bin = Path::new(validation_path).with_extension("bin");
    for path in [&training_bin, &validation_bin] {
        if !path.is_file() {
            eprintln!("Converted Bullet data not found: {}", path.display());
            eprintln!("Run .\\prepare_data.ps1 first.");
            std::process::exit(2);
        }
    }

    let training_positions = match count_lines(data_path) {
        Ok(count) if count > 0 => count,
        Ok(_) => {
            eprintln!("Training data is empty: {data_path}");
            std::process::exit(2);
        }
        Err(error) => {
            eprintln!("Could not read training data {data_path}: {error}");
            std::process::exit(2);
        }
    };
    let validation_positions = match count_lines(validation_path) {
        Ok(count) if count > 0 => count,
        Ok(_) => {
            eprintln!("Validation data is empty: {validation_path}");
            std::process::exit(2);
        }
        Err(error) => {
            eprintln!("Could not read validation data {validation_path}: {error}");
            std::process::exit(2);
        }
    };

    let output_directory = args.get(2).map(String::as_str).unwrap_or("checkpoints");
    let wdl_mix = args
        .get(3)
        .map_or(Ok(0.5f32), |value| value.parse::<f32>())
        .unwrap_or_else(|_| {
            eprintln!("WDL mix must be a number from 0.0 to 1.0.");
            std::process::exit(2);
        });
    if !(0.0..=1.0).contains(&wdl_mix) {
        eprintln!("WDL mix must be between 0.0 and 1.0.");
        std::process::exit(2);
    }
    let batches_per_superbatch = training_positions.div_ceil(BATCH_SIZE);
    println!("Training positions: {training_positions}");
    println!("Validation positions: {validation_positions}");
    println!("Batches per superbatch: {batches_per_superbatch}");
    println!("WDL result weight: {wdl_mix:.2}");

    let mut trainer = ValueTrainerBuilder::default()
        .dual_perspective()
        .optimiser(AdamW)
        .inputs(ChessBucketsMirrored::new(BUCKET_LAYOUT))
        .save_format(&[
            SavedFormat::id("l0w")
                .transform(|store, weights| {
                    let factoriser = store.get("l0f").values.f32().repeat(NUM_INPUT_BUCKETS);
                    weights
                        .into_iter()
                        .zip(factoriser)
                        .map(|(weight, shared)| weight + shared)
                        .collect()
                })
                .round()
                .quantise::<i16>(QA),
            SavedFormat::id("l0b").round().quantise::<i16>(QA),
            SavedFormat::id("l1w").round().quantise::<i16>(QB),
            SavedFormat::id("l1b").round().quantise::<i16>(QA * QB),
        ])
        .loss_fn(|output, target| output.sigmoid().squared_error(target))
        .build(|builder, stm_inputs, ntm_inputs| {
            let factoriser =
                builder.new_weights("l0f", Shape::new(HIDDEN, BASE_INPUTS), InitSettings::Zeroed);
            let expanded_factoriser = factoriser.repeat(NUM_INPUT_BUCKETS);
            let mut first = builder.new_affine("l0", NUM_INPUTS, HIDDEN);
            first.weights = first.weights + expanded_factoriser;
            let output = builder.new_affine("l1", 2 * HIDDEN, 1);
            let stm = first.forward(stm_inputs).screlu();
            let ntm = first.forward(ntm_inputs).screlu();
            output.forward(stm.concat(ntm))
        });

    let schedule = TrainingSchedule {
        net_id: format!(
            "valox-768x4hm-512-wdl{:02}",
            (wdl_mix * 100.0).round() as u8
        ),
        eval_scale: EVAL_SCALE,
        steps: TrainingSteps {
            batch_size: BATCH_SIZE,
            batches_per_superbatch,
            start_superbatch: 1,
            end_superbatch: SUPERBATCHES,
        },
        wdl_scheduler: wdl::ConstantWDL { value: wdl_mix },
        lr_scheduler: lr::CosineDecayLR {
            initial_lr: 0.001,
            final_lr: 0.00003,
            final_superbatch: SUPERBATCHES,
        },
        save_rate: 5,
    };

    let settings = LocalSettings {
        threads: 4,
        // This pinned Bullet revision accepts a test_set but explicitly does not evaluate it.
        // Holdout metrics are computed by the separate `validate` binary.
        test_set: None,
        output_directory,
        batch_queue_size: 16,
    };
    let data_loader = DirectSequentialDataLoader::new(&[training_bin
        .to_str()
        .expect("data path must be valid UTF-8")]);

    println!("Training an experimental Valox Chess768 NNUE.");
    println!("The current Reckless v60 network is not modified by this tool.");
    trainer.run(&schedule, &settings, &data_loader);
}

#[cfg(feature = "cuda")]
fn count_lines(path: &str) -> std::io::Result<usize> {
    BufReader::new(File::open(path)?)
        .lines()
        .try_fold(0usize, |count, line| line.map(|_| count + 1))
}
