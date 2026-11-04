// Copyright 2026 Joseph Anthony Abbott III
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! `hardshell-plan`: from syntax tree to physical plan.
//!
//! The binder, logical plans, the optimizer, statistics and physical
//! planning. Design: `docs/subsystems/planner.md`.

// Doc examples get none of the workspace's lints: keep `unsafe` out of them
// (ADR-0013; `cargo xtask unsafe-audit` checks for this attribute).
#![doc(test(attr(forbid(unsafe_code))))]
