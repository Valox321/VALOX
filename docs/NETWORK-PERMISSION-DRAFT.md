# Draft permission request: Reckless v60 NNUE weights

**Status: not sent.** Valox uses `networks/v60-7f587dfb.nnue`
(Reckless v60) as optional evaluation weights. The engine now has a
weight-free build that loads a compatible network at runtime through the UCI
`EvalFile` option, so a legal release no longer depends on this answer. The
question is whether the weights may also be embedded in a single-file binary.
The Reckless engine repository is licensed under AGPL-3.0, but the separate
`https://github.com/codedeliveryservice/RecklessNetworks` repository does not
state a model-specific license. Until the rights are clarified, Valox binaries
and source snapshots that contain this file must not be distributed.

Suggested recipient: the Reckless / RecklessNetworks maintainer. The
RecklessNetworks repository has issues disabled, so the available public
channel is the Reckless engine issue tracker:
`https://github.com/codedeliveryservice/Reckless/issues`. The question should
explicitly cover the separate networks repository; the engine license alone
does not establish terms for the weight files.

**Subject:** Permission to redistribute the Reckless v60 NNUE weights with Valox

Hello,

I maintain Valox, an independent UCI chess engine written in Rust. Valox has
its own search, move generation, and NNUE inference code. A current local build
can embed the Reckless v60 network file (`v60-7f587dfb.nnue`) as its evaluation
weights; the public release is weight-free and asks the user to load the file
through a UCI option.

I saw that the Reckless engine is published under AGPL-3.0, while the separate
RecklessNetworks repository does not show a model-specific license. I would
like to ask under what terms, if any, the v60 weights may be redistributed:

1. Are the v60 weight files covered by the Reckless engine's AGPL-3.0 license,
   or by a different license?
2. May the weights be bundled with Valox binaries and source archives?
3. If yes, which license and attribution should be used? For example, is an
   AGPL-3.0-compatible release acceptable, or would you prefer a separate
   written permission?
4. If redistribution is not permitted, may Valox document how users can
   obtain the weights themselves, without shipping the file?

Valox is not a Reckless fork and does not include Reckless engine source. The
network is used only as trained evaluation data. No Valox binary or source
snapshot containing the weights has been published; this request is to clarify
the terms before any stronger release.

If useful, I can provide the file checksum, the Valox NNUE inference code, and
a description of how the weights are embedded.

Thank you,
Valox maintainer

## Before sending

- Confirm the current recipient and contact channel on the RecklessNetworks
  repository; do not guess an address.
- Do not attach the weights or publish a binary containing them before a
  written answer arrives.
- Record the answer in `THIRD_PARTY_NOTICES.md`, `docs/RELEASE_CANDIDATE.md`,
  and `docs/CCC_READINESS.md`.
- If permission is denied or unanswered, either ship Valox without the weights
  (classical fallback) or replace them with a Valox-trained network that can be
  distributed under the chosen license.
