# Third-Party Notices

WHAT is licensed under the GNU Affero General Public License v3.0 or later (see `LICENSE`).
It includes or depends on the following third-party works.

## ccusage

- Source: https://github.com/ryoppippi/ccusage (commit `b71a1aae581b5961655b2cacf4aaf8c402981554`)
- Used as: Rust crate dependencies (`ccusage-core`, `ccusage-adapter-claude`,
  `ccusage-adapter-codex`, `ccusage-adapter-antigravity`, `ccusage-pricing-snapshots`)
  for usage log parsing, deduplication and cost calculation. The billing-block
  algorithm in `crates/what-core/src/blocks.rs`, and the Codex service-tier merge
  rule and Antigravity pricing lookup in `crates/what-sources/src/ccusage_backend.rs`
  are ported from ccusage.

```
MIT License

Copyright (c) 2025 ryoppippi

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## LiteLLM model pricing data

- Source: https://github.com/BerriAI/litellm — `model_prices_and_context_window.json`
  (commit `20d53f2951c79bb9164af9032588c8b0a561727b`, vendored at
  `vendor/pricing/litellm-pricing.json` and embedded at build time).
- License: MIT, Copyright (c) 2023 Berri AI.

## models.dev pricing data

- Embedded through `ccusage-pricing-snapshots` (snapshot maintained by ccusage).
- Source: https://github.com/anomalyco/models.dev — MIT License.

## Slint

- Source: https://slint.dev — used as the GUI toolkit (`slint`, `slint-build`).
- License: GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0.
  WHAT uses Slint under the GPL-3.0; section 13 of the GNU GPL v3 and the GNU AGPL v3
  permits combining the two in one work.

## SQLite

- Compiled into the binary through the `sqlite` / `sqlite3-src` crates (bundled build),
  used read-only to load Antigravity conversation databases.
- SQLite is in the public domain (https://sqlite.org/copyright.html).

## Other Rust crates

Other dependencies are distributed under MIT, Apache-2.0, BSD-2-Clause,
BSD-3-Clause, ISC, Zlib, BSL-1.0, Unicode-3.0, CC0-1.0, Unlicense,
CDLA-Permissive-2.0 (`webpki-roots`) or MPL-2.0 (`option-ext`, used unmodified
through `directories`). All are compatible with the AGPL-3.0. Inspect the full list with:

```
cargo metadata --format-version 1
```
