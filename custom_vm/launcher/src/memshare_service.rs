//
// Copyright (C) 2025 The Android Open-Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This module implements the IMemoryBufferShare Android side service.

use android_trusty_membuf::aidl::android::trusty::membuf::{
    IMemoryBufferContext::BnMemoryBufferContext, IMemoryBufferContext::IMemoryBufferContext,
    IMemoryBufferShare::BnMemoryBufferShare, IMemoryBufferShare::IMemoryBufferShare,
    IMemoryBufferShare::HOST_BUFFER, IMemoryBufferShare::SECURE_DISPLAY_FRAME_BUFFER,
    IMemoryBufferShare::SECURE_VIDEO_DECODER_INPUT, IMemoryBufferShareVm::IMemoryBufferShareVm,
    MemoryBufferToken::MemoryBufferToken, ShareMemoryBufferResult::ShareMemoryBufferResult,
};
#[cfg(android_vendor)]
use avf_bindgen::{
    AVirtualMachineMemoryMappingAttributes, AVirtualMachine_addMemoryMapping,
    AVirtualMachine_removeMemoryMapping,
};
use binder::{self, AccessorProvider, ParcelFileDescriptor, Status, StatusCode, Strong};
#[cfg(not(android_vendor))]
use std::fs::File;
#[cfg(android_vendor)]
use std::os::fd::AsRawFd;
use std::sync::{Mutex, OnceLock};

use thiserror::Error;

const SERVICE_NAME: &str = "android.trusty.membuf.IMemoryBufferShare/memshare_vm";
const VM_SERVICE_NAME: &str = "android.trusty.membuf.IMemoryBufferShareVm/memshare_vm";
const VM_ACCESSOR_NAME: &str = "android.os.IAccessor/IMemoryBufferShareVm/memshare_vm";

static ACCESSOR_PROVIDER: OnceLock<Option<AccessorProvider>> = OnceLock::new();
static MEMORY_SHARE_VM_SERVICE: OnceLock<Result<Strong<dyn IMemoryBufferShareVm>, MemShareError>> =
    OnceLock::new();
static MEMORY_MANAGER: OnceLock<Mutex<MemoryManager>> = OnceLock::new();

#[cfg(not(android_vendor))]
const ERROR_MAPPING_MEMORY: i32 = -1;

#[derive(Clone, Error, Debug)]
pub enum MemShareError {
    #[error("Port configuration error for port: {0}")]
    BinderError(String),
    #[error("Couldn't convert value")]
    ConversionError,
    #[error("Invalid value provided")]
    InvalidValue,
    #[error("Resources exhausted")]
    AllocationError,
    #[error("System was found on an invalid state")]
    InvalidState,
    #[error("Error found when mapping buffer")]
    BufferMappingProblem,
    #[error("An error occurred: {0}")]
    GenericError(String),
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum ProtectionId {
    HostBuffer,
    SecureDisplayFrameBuffer,
    SecureVideoDecoderInput,
}

impl TryFrom<i32> for ProtectionId {
    type Error = MemShareError;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            HOST_BUFFER => Ok(ProtectionId::HostBuffer),
            SECURE_DISPLAY_FRAME_BUFFER => Ok(ProtectionId::SecureDisplayFrameBuffer),
            SECURE_VIDEO_DECODER_INPUT => Ok(ProtectionId::SecureVideoDecoderInput),
            _ => {
                log::error!("Received invalid ProtectionId value: {value}");
                Err(MemShareError::ConversionError)
            }
        }
    }
}

#[derive(Clone, Debug)]
struct MemoryRange {
    protection_id: ProtectionId,
    range_ipa_start: u64,
    range_size: u64,
}

struct MemoryManager {
    buffers: Vec<(u64, u64)>,
    secure_buffers: Vec<(u64, u64)>,
}

impl MemoryManager {
    // For testing we will provide only 2 1 page buffers for now
    const HOST_BUFFER_1_BASE_ADDRESS: u64 = 0x8000_0000 + 64 * 1024 * 1024;
    const HOST_BUFFER_2_BASE_ADDRESS: u64 =
        Self::HOST_BUFFER_1_BASE_ADDRESS + Self::HOST_BUFFER_MAX_SUPPORTED_SIZE;
    const HOST_BUFFER_MAX_SUPPORTED_SIZE: u64 = 64 * 1024 * 1024;

    const BUFFERS_BASE_SIZE: u64 = 4096;

    const SECURE_BUFFER_BASE_ADDRESS: u64 = 0x9000_0000;
    const SECURE_BUFFER_MAX_SUPPORTED_SIZE: u64 = 64 * 1024 * 1024;

    fn new() -> Self {
        let buffers = vec![
            (Self::HOST_BUFFER_2_BASE_ADDRESS, Self::HOST_BUFFER_MAX_SUPPORTED_SIZE),
            (Self::HOST_BUFFER_1_BASE_ADDRESS, Self::HOST_BUFFER_MAX_SUPPORTED_SIZE),
        ];
        let secure_buffers =
            vec![(Self::SECURE_BUFFER_BASE_ADDRESS, Self::SECURE_BUFFER_MAX_SUPPORTED_SIZE)];
        Self { buffers, secure_buffers }
    }

    fn get_buffer_max_size(protection_id: ProtectionId) -> Result<u64, MemShareError> {
        match protection_id {
            ProtectionId::HostBuffer => Ok(Self::HOST_BUFFER_MAX_SUPPORTED_SIZE),
            ProtectionId::SecureDisplayFrameBuffer => Ok(Self::SECURE_BUFFER_MAX_SUPPORTED_SIZE),
            _ => {
                log::error!(
                    "Only HostBuffer and SecureDisplayFrameBuffer are supported, received: {:?}",
                    protection_id
                );
                Err(MemShareError::InvalidValue)
            }
        }
    }

    fn get_buffer_reference(
        &mut self,
        protection_id: ProtectionId,
    ) -> Result<(u64, &mut Vec<(u64, u64)>), MemShareError> {
        let max_supported_size = Self::get_buffer_max_size(protection_id)?;
        match protection_id {
            ProtectionId::HostBuffer => Ok((max_supported_size, &mut self.buffers)),
            ProtectionId::SecureDisplayFrameBuffer => {
                Ok((max_supported_size, &mut self.secure_buffers))
            }
            _ => {
                log::error!(
                    "Only HostBuffer and SecureDisplayFrameBuffer are supported, received: {:?}",
                    protection_id
                );
                Err(MemShareError::InvalidValue)
            }
        }
    }

    fn add_area(
        &mut self,
        protection_id: ProtectionId,
        base_address: u64,
        size_bytes: u64,
    ) -> Result<(), MemShareError> {
        let (supported_size, buffers) = self.get_buffer_reference(protection_id)?;

        if size_bytes > supported_size {
            log::error!(
                "add_area: Only buffers of size up to {} are supported, for protection id {:?}; received {}",
                supported_size,
                protection_id,
                size_bytes
            );
            return Err(MemShareError::InvalidValue);
        }
        // TODO: We need to check for overlaps with existing areas, overflow on the addresses,
        //       and that base addresses match protection IDs
        buffers.push((base_address, size_bytes));

        Ok(())
    }

    fn get_map_area(
        &mut self,
        size_bytes: i32,
        protection_id: ProtectionId,
    ) -> Result<(u64, u64), MemShareError> {
        let (supported_size, buffers) = self.get_buffer_reference(protection_id)?;

        if (size_bytes < 0) || (size_bytes as u64 > supported_size) {
            log::error!(
                "get_map_area: Only buffers of size up to {} are supported for protection ID {:?}, received {}",
                supported_size,
                protection_id,
                size_bytes
            );
            return Err(MemShareError::InvalidValue);
        }
        if !(size_bytes as u64).is_multiple_of(Self::BUFFERS_BASE_SIZE) {
            log::error!(
                "size must be multiple of {}, received {}",
                Self::BUFFERS_BASE_SIZE,
                size_bytes
            );
            return Err(MemShareError::InvalidValue);
        }
        if buffers.is_empty() {
            log::error!("No more buffers available for pretection ID {:?}", protection_id);
            return Err(MemShareError::AllocationError);
        }
        // buffer is not empty, so we can unwrap
        let buffer = buffers.pop().unwrap();
        Ok((buffer.0, buffer.0 + (size_bytes as u64)))
    }
}

struct MemoryBufferContextData {
    dma_buffer_id: Option<i32>,
    memory_range: Option<MemoryRange>,
    ta_memory_buffer_ctx: Option<Strong<dyn IMemoryBufferContext>>,
}

#[cfg(android_vendor)]
fn remove_memory_mapping(dma_buffer_id: i32) -> Result<(), MemShareError> {
    let vm = crate::MEMSHARE_VM.get().ok_or_else(|| {
        log::error!("MemShare VM object was not available");
        MemShareError::InvalidState
    })?;
    let vm_obj = vm
        .lock()
        .map_err(|e| {
            log::error!("Couldn't lock vm object: {:?}", e);
            MemShareError::InvalidState
        })?
        .0;
    // SAFETY: vm_obj is a valid pointer returned by AVirtualMachine_createRaw.dma_buffer_id doesn't
    // affect the safety of the call.
    let mapping_removed = unsafe { AVirtualMachine_removeMemoryMapping(vm_obj, dma_buffer_id) };
    if !mapping_removed {
        log::error!("couldn't remove DMA buffer");
        return Err(MemShareError::BufferMappingProblem);
    };
    Ok(())
}

#[cfg(not(android_vendor))]
fn remove_memory_mapping(dma_buffer_id: i32) -> Result<(), MemShareError> {
    let vm = crate::MEMSHARE_VM.get().ok_or_else(|| {
        log::error!("MemShare VM object was not available");
        MemShareError::InvalidState
    })?;
    vm.remove_memory_mapping(dma_buffer_id).map_err(|e| {
        log::error!("couldn't remove DMA buffer: {:?}", e);
        MemShareError::BufferMappingProblem
    })?;
    Ok(())
}

impl MemoryBufferContextData {
    fn new(
        dma_buffer_id: i32,
        protection_id: ProtectionId,
        range_ipa_start: u64,
        range_size: u64,
    ) -> Self {
        let memory_range = MemoryRange { protection_id, range_ipa_start, range_size };
        MemoryBufferContextData {
            dma_buffer_id: Some(dma_buffer_id),
            memory_range: Some(memory_range),
            ta_memory_buffer_ctx: None,
        }
    }

    fn get_ipa(&self) -> Result<i64, MemShareError> {
        let memory_range = self.memory_range.as_ref().ok_or(MemShareError::InvalidState)?;
        // The values we are using for our memory map will not overflow an i64
        Ok(memory_range.range_ipa_start as i64)
    }

    fn free_resources(&mut self) -> Result<(), MemShareError> {
        if let Some(ta_memory_buffer_ctx) = self.ta_memory_buffer_ctx.take() {
            ta_memory_buffer_ctx.releaseMemoryBufferContext().map_err(|e| {
                log::error!("TA releaseMemoryBufferContext call failed: {:?}", e);
                MemShareError::GenericError("TA releaseMemoryBufferContext call failed".to_string())
            })?;
        }
        if let Some(dma_buffer_id) = self.dma_buffer_id.take() {
            remove_memory_mapping(dma_buffer_id)?;
        }
        if let Some(memory_range) = self.memory_range.take() {
            let range_max_size = MemoryManager::get_buffer_max_size(memory_range.protection_id)?;
            // MemoryBufferContext are only created after MEMORY_MANAGER is initialized
            let memory_manager = MEMORY_MANAGER.get().ok_or_else(|| {
                log::error!("Memory Manager was not available");
                MemShareError::InvalidState
            })?;
            memory_manager
                .lock()
                .map_err(|_| {
                    log::error!("found a poisoned memory_manager mutex on memory_manager");
                    MemShareError::InvalidState
                })?
                .add_area(memory_range.protection_id, memory_range.range_ipa_start, range_max_size)
                .map_err(|e| {
                    log::error!(
                        "couldn't add area back: {:?} at address 0x{:x} of size {}",
                        e,
                        memory_range.range_ipa_start,
                        memory_range.range_size
                    );
                    MemShareError::GenericError(
                        "Coudln't return memory range to manager".to_string(),
                    )
                })?;
        }
        Ok(())
    }

    fn get_portable_token(&self) -> binder::Result<MemoryBufferToken> {
        let mem_buff_ctx = self.ta_memory_buffer_ctx.as_ref().ok_or_else(|| {
            log::error!("We do not have a TA IMemoryBufferContext to call");
            binder::Status::new_exception(
                binder::ExceptionCode::ILLEGAL_STATE,
                Some(c"Couldn't get TA IMemoryBufferContext"),
            )
        })?;
        mem_buff_ctx.getPortableToken()
    }
}

impl Drop for MemoryBufferContextData {
    fn drop(&mut self) {
        let e = self.free_resources();
        if e.is_err() {
            log::error!("MemoryBufferContextData drop encountered an error: {:?}", e);
        }
    }
}

struct MemoryBufferContext {
    memory_buffer_data: Mutex<MemoryBufferContextData>,
}

impl binder::Interface for MemoryBufferContext {}

impl MemoryBufferContext {
    fn new(
        dma_buffer_id: i32,
        protection_id: ProtectionId,
        range_ipa_start: u64,
        range_size: u64,
    ) -> Self {
        let memory_buffer_data =
            MemoryBufferContextData::new(dma_buffer_id, protection_id, range_ipa_start, range_size);
        MemoryBufferContext { memory_buffer_data: Mutex::new(memory_buffer_data) }
    }

    fn new_binder(
        self,
        ta_memory_buffer_ctx: Strong<dyn IMemoryBufferContext>,
    ) -> Strong<dyn IMemoryBufferContext> {
        self.memory_buffer_data.lock().expect("poisoned mutex").ta_memory_buffer_ctx =
            Some(ta_memory_buffer_ctx);
        BnMemoryBufferContext::new_binder(self, binder::BinderFeatures::default())
    }

    fn get_ipa(&self) -> Result<i64, MemShareError> {
        self.memory_buffer_data.lock().expect("poisoned mutex").get_ipa()
    }

    fn free_resources(&mut self) -> Result<(), MemShareError> {
        self.memory_buffer_data.lock().expect("poisoned mutex").free_resources()
    }
}

impl IMemoryBufferContext for MemoryBufferContext {
    fn getPortableToken(&self) -> binder::Result<MemoryBufferToken> {
        self.memory_buffer_data.lock().expect("poisoned mutex").get_portable_token()
    }

    fn releaseMemoryBufferContext(&self) -> Result<i32, Status> {
        self.memory_buffer_data.lock().expect("poisoned mutex").free_resources().map_err(|e| {
            log::error!("Couldn't free resources after TA released memory buffer context: {:?}", e);
            binder::Status::new_exception(
                binder::ExceptionCode::ILLEGAL_STATE,
                Some(c"Couldn't free resources after TA released memory buffer context"),
            )
        })?;
        Ok(0)
    }
}

fn get_mem_share_vm_service() -> Result<Strong<dyn IMemoryBufferShareVm>, MemShareError> {
    let _accessor_provider = ACCESSOR_PROVIDER
        .get_or_init(|| {
            AccessorProvider::new(&[VM_SERVICE_NAME.to_owned()], |s| {
                binder::wait_for_service(VM_ACCESSOR_NAME)
                    .and_then(|service| binder::Accessor::from_binder(s, service))
            })
        })
        .as_ref()
        .ok_or(MemShareError::BinderError("failed to create accessor provider".to_string()));
    MEMORY_SHARE_VM_SERVICE
        .get_or_init(|| {
            binder::wait_for_interface(VM_SERVICE_NAME).map_err(|_| {
                MemShareError::BinderError("Couldn't get IMemoryShareVM service".to_string())
            })
        })
        .clone()
}

struct MemoryBufferShare;

impl binder::Interface for MemoryBufferShare {}

impl IMemoryBufferShare for MemoryBufferShare {
    fn shareMemoryBuffer(
        &self,
        fd: &ParcelFileDescriptor,
        size_bytes: i32,
        protection_id: i32,
    ) -> binder::Result<ShareMemoryBufferResult> {
        let mem_share_vm_service = get_mem_share_vm_service().map_err(|e| {
            log::error!("Couldn't get mem share service: {:?}", e);
            binder::Status::new_exception(
                binder::ExceptionCode::ILLEGAL_STATE,
                Some(c"Couldn't get mem share service"),
            )
        })?;
        let mut mem_buf_ctx = map_memory_buffer(fd, size_bytes, protection_id).map_err(|e| {
            log::error!("Couldn't map memory buffer: {:?}", e);
            binder::Status::new_exception(
                binder::ExceptionCode::ILLEGAL_STATE,
                Some(c"Couldn't map memory buffer"),
            )
        })?;
        let ipa = mem_buf_ctx.get_ipa().map_err(|e| {
            log::error!("Get IPA failed: {:?}", e);
            binder::Status::new_exception(
                binder::ExceptionCode::ILLEGAL_STATE,
                Some(c"Couldn't get IPA for range"),
            )
        })?;
        let ta_memory_buffer_ctx = mem_share_vm_service
            .shareMemoryBuffer(ipa, size_bytes)
            .map_err(|e| {
                log::error!("shareMemoryBuffer in TA failed: {:?}", e);
                let free_res = mem_buf_ctx.free_resources();
                if free_res.is_err() {
                    log::error!("Free resources failed: {:?}", free_res);
                }
                e
            })?
            .context
            .ok_or_else(|| {
                log::error!("Received a null Memory Buffer CTX");
                binder::Status::new_exception(
                    binder::ExceptionCode::NULL_POINTER,
                    Some(c"TA returned a NULL ptr as Memory context"),
                )
            })?;
        Ok(ShareMemoryBufferResult { context: Some(mem_buf_ctx.new_binder(ta_memory_buffer_ctx)) })
    }
}

impl MemoryBufferShare {
    fn new_binder() -> Strong<dyn IMemoryBufferShare> {
        BnMemoryBufferShare::new_binder(MemoryBufferShare, binder::BinderFeatures::default())
    }
}

pub(crate) fn register_memshare_service() -> Result<(), StatusCode> {
    binder::add_service(SERVICE_NAME, MemoryBufferShare::new_binder().as_binder())
}

#[cfg(android_vendor)]
fn add_memory_mapping(
    fd: &ParcelFileDescriptor,
    range_ipa_start: u64,
    range_ipa_end: u64,
) -> Result<i32, MemShareError> {
    let vm = crate::MEMSHARE_VM.get().ok_or_else(|| {
        log::error!("MemShare VM object was not available");
        MemShareError::InvalidState
    })?;
    let vm_obj = vm
        .lock()
        .map_err(|e| {
            log::error!("Couldn't lock vm object: {:?}", e);
            MemShareError::InvalidState
        })?
        .0;
    // SAFETY: vm_obj is a valid pointer returned by AVirtualMachine_createRaw
    let dma_buffer_id = unsafe {
        AVirtualMachine_addMemoryMapping(vm_obj, fd.as_raw_fd(), range_ipa_start, range_ipa_end, 0, AVirtualMachineMemoryMappingAttributes::AVIRTUAL_MACHINE_MEMORY_MAPPING_ATTRIBUTE_CACHE_COHERENT)
    };
    Ok(dma_buffer_id)
}

#[cfg(not(android_vendor))]
fn add_memory_mapping(
    fd: &ParcelFileDescriptor,
    range_ipa_start: u64,
    range_ipa_end: u64,
) -> Result<i32, MemShareError> {
    let file = get_file_from_fd(fd)?;
    let vm = crate::MEMSHARE_VM.get().ok_or_else(|| {
        log::error!("MemShare VM object was not available");
        MemShareError::InvalidState
    })?;
    match vm.add_memory_mapping(file, range_ipa_start, range_ipa_end, 0, true) {
        Ok(dma_buffer_id) => Ok(dma_buffer_id),
        Err(e) => {
            log::error!("Error received when mapping buffer: {:?}", e);
            // Will return a negative number so caller will do cleanup before returning error
            Ok(ERROR_MAPPING_MEMORY)
        }
    }
}

fn map_memory_buffer(
    fd: &ParcelFileDescriptor,
    size_bytes: i32,
    protection_id: i32,
) -> Result<MemoryBufferContext, MemShareError> {
    // TODO: Hardcoded values will need to match what is provided to TA
    if size_bytes < 0 {
        log::error!("size_bytes was negative: {size_bytes}");
        return Err(MemShareError::InvalidValue);
    }
    let protection_id: ProtectionId = protection_id.try_into()?;
    let memory_manager = MEMORY_MANAGER.get_or_init(|| Mutex::new(MemoryManager::new()));
    let (range_ipa_start, range_ipa_end) =
        memory_manager.lock().expect("poisoned mutex").get_map_area(size_bytes, protection_id)?;
    log::info!("Mapping fd {fd:?} at start: {range_ipa_start:#x} end: {range_ipa_end:#x}");
    let dma_buffer_id = add_memory_mapping(fd, range_ipa_start, range_ipa_end)?;
    if dma_buffer_id < 0 {
        log::error!("Couldn't add dma buffer. Received: {}", dma_buffer_id);
        memory_manager.lock().expect("poisoned mutex").add_area(
            protection_id,
            range_ipa_start,
            range_ipa_end - range_ipa_start,
        )?;
        return Err(MemShareError::BufferMappingProblem);
    }
    let mem_buf_ctx =
        MemoryBufferContext::new(dma_buffer_id, protection_id, range_ipa_start, size_bytes as u64);
    Ok(mem_buf_ctx)
}

#[cfg(not(android_vendor))]
fn get_file_from_fd(fd: &ParcelFileDescriptor) -> Result<File, MemShareError> {
    let dup_fd = fd.as_ref().try_clone().map_err(|e| {
        log::error!("Couldn't duplicate pfd: {:?}", e);
        MemShareError::InvalidValue
    })?;
    Ok(File::from(dup_fd))
}
