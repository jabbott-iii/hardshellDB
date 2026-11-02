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

//! `hardshell-types`: SQL types and values.
//!
//! Text input and output, comparison, casts and arithmetic for every SQL
//! type, `NUMERIC`, dates and times, and the two byte encodings the storage
//! engine sees: the order-preserving key encoding and the row encoding.
//! Design: `docs/subsystems/types-and-values.md`.
