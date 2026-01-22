// Copyright 2025, The Android Open Source Project
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
//! vendor_trusty_vm_launcher
//use rustutils::android::system_properties;
use anyhow::{bail, ensure, Context, Result};
use avf_bindgen::{
    AVirtualMachine, AVirtualMachineRawConfig_create, AVirtualMachineRawConfig_setInstanceId,
    AVirtualMachineRawConfig_setKernel, AVirtualMachineRawConfig_setMemoryMiB,
    AVirtualMachineRawConfig_setName, AVirtualMachineRawConfig_setProtectedVm,
    AVirtualMachine_addAccessor, AVirtualMachine_createRaw, AVirtualMachine_destroy,
    AVirtualMachine_start, AVirtualizationService_create, AVirtualizationService_destroy,
};
use binder::{self, ProcessState};
use clap::Parser;
use env_logger::Builder;
use hypervisor_props::is_protected_vm_supported;
use log::{info, warn, LevelFilter};
use serde::Deserialize;
use std::ffi::CString;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::{AsRawFd, IntoRawFd};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

mod memshare_service;

struct AVMWrapper(*mut AVirtualMachine);

// SAFETY: Application is single threaded, so no transfers to different threads will happen.
// Addtionally AVirtualMachine, underneath the rust implementation *AVirtualMachine is a
// *VmInstance which can be send between threads.
unsafe impl Send for AVMWrapper {}

pub(crate) static MEMSHARE_VM: OnceLock<Mutex<AVMWrapper>> = OnceLock::new();

const INSTANCE_ID_SIZE: usize = 64;
#[derive(Parser, Debug)]
/// Collection of CLI for vendor_trusty_vm_launcher
pub struct Args {
    /// Path to the trusty kernel image.
    #[arg(long)]
    kernel: PathBuf,
    /// Whether the VM is protected or not.
    #[arg(long)]
    protected: bool,
    /// Name of the VM.
    #[arg(long, default_value = "vendor_vm")]
    name: String,
    /// Memory size of the VM in MiB
    #[arg(long, default_value_t = 128)]
    memory_size_mib: i32,
    /// Path to a JSON file defining the RPC services to register.
    #[arg(long, value_name = "FILE")]
    rpc_services_config: Vec<PathBuf>,
    /// If enabled, allow this VM to access FF-A. The launching process must
    /// have CAP_IPC_OWNER and be configured by selinux to use guest_ffa_tee_service.
    /// This is only settable on a protected vm (enforced by virtmgr).
    #[arg(long)]
    allow_ffa: bool,
    /// Path to a file containing the VM instance ID.
    #[arg(long, value_name = "FILE")]
    vm_instance_id: Option<PathBuf>,
}

fn main() -> Result<()> {
    info!("memshare vendor launcher started");
    let args = Args::parse();
    const BINARY_NAME: &str = "vendor_trusty_vm_launcher";
    let vm_name = args.name.to_owned();
    Builder::new()
        // Set the default log level if not configured via RUST_LOG
        .filter_level(LevelFilter::Info)
        .format(move |buf, record| {
            writeln!(
                buf,
                // Format: "[LEVEL] binary_name:vm_name: log_message"
                "[{}] {}:{}: {}",
                record.level(),
                BINARY_NAME,
                vm_name,
                record.args()
            )
        })
        .init();
    let protected_vm = if is_protected_vm_supported().unwrap_or(false) {
        args.protected
    } else {
        if args.protected {
            warn!("protected VM is not supported; launch non-protected VM");
        }
        false
    };
    let instance_id = if let Some(path) = args.vm_instance_id.as_ref() {
        info!("Loading VM Instance ID from file: {path:?}");
        load_instance_id(path)?
    } else {
        warn!("No VM Instance ID file provided. Using default instance ID.");
        let mut instance_id: [u8; 64] = [0; 64];
        instance_id[0..4].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
        instance_id
    };
    let console_out = create_log_writer(&args.name)?;
    let log_out = console_out.try_clone().context("Failed to clone console_out fd for log_out")?;
    // SAFETY: AVirtualMachineRawConfig_create() isn't unsafe but rust_bindgen forces it to be seen
    // as unsafe
    let config = unsafe { AVirtualMachineRawConfig_create() };
    let c_str_name =
        CString::new(args.name).expect("CString::new failed: string contained internal null byte");
    let kernel_file = File::open(args.kernel).context("Failed to open kernel file")?;
    let kernel_fd = kernel_file.into_raw_fd();
    // SAFETY: config is the only reference to a valid object
    unsafe {
        AVirtualMachineRawConfig_setName(config, c_str_name.as_ptr());
        AVirtualMachineRawConfig_setKernel(config, kernel_fd);
        AVirtualMachineRawConfig_setProtectedVm(config, protected_vm);
        AVirtualMachineRawConfig_setMemoryMiB(config, args.memory_size_mib);
        AVirtualMachineRawConfig_setInstanceId(config, instance_id.as_ptr(), instance_id.len());
    }
    let mut vm = std::ptr::null_mut();
    let mut service = std::ptr::null_mut();
    ensure!(
        // SAFETY: &mut service is a valid non-null pointer to a mutable raw pointer.
        unsafe { AVirtualizationService_create(&mut service, false) } == 0,
        "AVirtualizationService_create failed"
    );
    scopeguard::defer! {
        // SAFETY: service is a valid pointer to AVirtualizationService
        unsafe { AVirtualizationService_destroy(service); }
    }
    ensure!(
        // SAFETY: &mut vm is a valid pointer to *AVirtualMachine
        unsafe {
            AVirtualMachine_createRaw(
                service,
                config,
                console_out.as_raw_fd(), // console_out
                -1,                      // console_in
                log_out.as_raw_fd(),     // log
                &mut vm,
            )
        } == 0,
        "AVirtualMachine_createRaw failed"
    );
    let _vm = MEMSHARE_VM.get_or_init(|| Mutex::new(AVMWrapper(vm)));
    scopeguard::defer! {
        // SAFETY: vm_obj contains a valid pointer to AVirtualMachine
        unsafe {
            let vm_obj = MEMSHARE_VM
                .get()
                .expect("Should not happen, VM object has already been initialized")
                .lock()
                .expect("poisoned mutex, shouldn't happen on a singled threaded application")
                .0;
            AVirtualMachine_destroy(vm_obj);
        }
    }
    info!("vm created");
    {
        let vm_obj = MEMSHARE_VM
            .get()
            .expect("Should not happen, VM object has already been initialized")
            .lock()
            .expect("poisoned mutex, shouldn't happen on a singled threaded application")
            .0;
        // SAFETY: vm_obj contains the only reference to a valid object
        unsafe {
            AVirtualMachine_start(vm_obj);
        }
    }
    info!("VM started");
    ProcessState::start_thread_pool();
    if !args.rpc_services_config.is_empty() {
        for config_path in args.rpc_services_config {
            let configs = parse_rpc_service_configs(&config_path)?;
            ensure!(
                !configs.is_empty(),
                "RPC services config file at '{:?}' is empty",
                config_path
            );
            info!("Registering {} RPC service(s) from {}...", configs.len(), config_path.display());
            for config in &configs {
                let rpc_service_name_cstr =
                    CString::new(config.internal_rpc_service_name.as_str())?;
                let accessor_name_cstr = CString::new(config.accessor_name.as_str())?;
                info!(
                    "Adding service {} from  accessor {} on port {}...",
                    config.internal_rpc_service_name.as_str(),
                    config.accessor_name.as_str(),
                    config.port
                );
                ensure!(
                    {
                        let vm_obj = MEMSHARE_VM
                            .get()
                            .expect("Should not happen, VM object has already been initialized")
                            .lock()
                            .expect("poisoned mutex, shouldn't happen on singled threaded app")
                            .0;
                        // SAFETY: vm_obj is a valid pointer returned by AVirtualMachine_createRaw.
                        // rpc_service_name_cstr and accessor_name_cstr are valid null terminated C
                        // strings.
                        unsafe {
                            AVirtualMachine_addAccessor(
                                vm_obj,
                                rpc_service_name_cstr.as_ptr(),
                                accessor_name_cstr.as_ptr(),
                                config.port,
                            )
                        }
                    } == 0,
                    "AVirtualMachine_addAccessor failed"
                );
            }
        }
    }
    memshare_service::register_memshare_service().context("Couldn't register memshare service")?;
    ProcessState::join_thread_pool();
    bail!("Thread pool unexpectedly ended");
}

fn load_instance_id(path: &Path) -> Result<[u8; INSTANCE_ID_SIZE]> {
    let mut file =
        File::open(path).with_context(|| format!("open VM Instance ID file: {:?}", path))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("get metadata for VM Instance ID file: {:?}", path))?;
    ensure!(
        metadata.len() == INSTANCE_ID_SIZE as u64,
        "VM Instance ID file {:?} has incorrect size. Expected {}, Got {}",
        path,
        INSTANCE_ID_SIZE,
        metadata.len()
    );
    let mut buffer = [0u8; INSTANCE_ID_SIZE];
    file.read_exact(&mut buffer)
        .with_context(|| format!("read VM Instance ID file: {:?}", path))?;
    Ok(buffer)
}

/// Defines the structure of a single RPC service configuration in the JSON file.
#[derive(Deserialize, Debug)]
struct RpcServiceConfig {
    port: i32,
    accessor_name: String,
    internal_rpc_service_name: String,
}
/// Parses a JSON file containing an array of RPC service configurations.
fn parse_rpc_service_configs(path: &Path) -> Result<Vec<RpcServiceConfig>> {
    let file =
        File::open(path).with_context(|| format!("open RPC services config at '{path:?}'"))?;
    serde_json::from_reader(file).with_context(|| format!("parse JSON from '{path:?}'"))
}

/// Creates a pipe and spawns a thread to forward the VM's output to stdout.
fn create_log_writer(prefix: &str) -> Result<File> {
    let (reader_fd, writer_fd) = nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)
        .context("Failed to create pipe for VM output")?;
    let reader = File::from(reader_fd);
    let writer = File::from(writer_fd);

    std::thread::Builder::new()
        .name(format!("vm-log-{prefix}"))
        .spawn(move || {
            let reader = BufReader::new(reader);
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                // Prefix guest logs to distinguish them from launcher logs
                info!("vm: {line}");
            }
        })
        .context("Failed to spawn VM logging thread")?;
    Ok(writer)
}
