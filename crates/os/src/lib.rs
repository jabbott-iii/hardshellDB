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

//! `hardshell-os`: the only crate that touches the operating system.
//!
//! The `Vfs` trait with `StdVfs`, `MemVfs` and the fault-injecting
//! `FaultVfs` (feature `testing`), file locks and sync rules. Every other
//! crate does file I/O through it. Design: `docs/subsystems/os-and-vfs.md`
//! and `docs/03-platforms.md`.
