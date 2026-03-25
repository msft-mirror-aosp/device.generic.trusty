// Copyright 2026, The Android Open Source Project
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

//! Library for Trusty VM launcher memshare service.

use binder::ParcelFileDescriptor;
use std::sync::OnceLock;

pub mod memshare_service;

pub use memshare_service::MemShareError;

/// Trait to abstract VM operations needed by the memshare service.
pub trait MemShareVm {
    /// Add a memory mapping to the VM.
    fn add_memory_mapping(
        &self,
        fd: &ParcelFileDescriptor,
        range_ipa_start: u64,
        range_ipa_end: u64,
    ) -> Result<i32, MemShareError>;

    /// Remove a memory mapping from the VM.
    fn remove_memory_mapping(&self, dma_buffer_id: i32) -> Result<(), MemShareError>;
}

/// Global VM instance used by the memshare service.
pub static MEMSHARE_VM: OnceLock<Box<dyn MemShareVm + Send + Sync>> = OnceLock::new();

/// Set the global VM instance.
pub fn set_vm_instance(
    vm: Box<dyn MemShareVm + Send + Sync>,
) -> Result<(), Box<dyn MemShareVm + Send + Sync>> {
    MEMSHARE_VM.set(vm)
}
