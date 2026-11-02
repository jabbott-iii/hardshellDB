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

//! `hardshell-core`: foundation types shared by every hardshellDB crate.
//!
//! SQLSTATE codes and errors, CRC-32C, varints, hashing, `Pcg32`, logging,
//! the property-test harness, the CLI argument parser and the CSV codec.
//! It depends on no other crate. Design: `docs/subsystems/core.md`.

pub mod error;
pub mod sqlstate;
