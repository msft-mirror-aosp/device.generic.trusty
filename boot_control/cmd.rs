//! Command-line tool for calling IBootControl

use android_hardware_boot::aidl::android::hardware::boot::{
    IBootControl::IBootControl, MergeStatus::MergeStatus,
};
use binder::{wait_for_interface, Status};
use clap::{builder::TypedValueParser, Parser, Subcommand};
use kernlog::KernelLog;
use log::{debug, error, info};
use std::process::ExitCode;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Command,

    #[arg(short, long, value_name = "MINIMUM", default_value_t = log::LevelFilter::Debug)]
    log_level: log::LevelFilter,
}

#[derive(Subcommand, Clone)]
#[command(rename_all = "camelCase")]
enum Command {
    GetActiveBootSlot,
    GetCurrentSlot,
    GetNumberSlots,
    GetSuffix {
        slot_index: i32,
    },
    IsSlotBootable {
        slot_index: i32,
    },
    IsSlotMarkedSuccessful {
        slot_index: i32,
    },
    SetSlotAsUnbootable {
        slot_index: i32,
    },
    SetActiveBootSlot {
        slot_index: i32,
    },
    MarkBootSuccessful,
    GetSnapshotMergeStatus,
    SetSnapshotMergeStatus {
        #[arg(value_parser = clap::builder::PossibleValuesParser::new([
                "NONE", "UNKNOWN", "SNAPSHOTTED", "MERGING", "CANCELLED"
            ]).map(parse_merge_status))]
        status: MergeStatus,
    },
}

impl Command {
    fn call(&self, bootctl: &(impl IBootControl + ?Sized)) -> Result<(), Status> {
        match self {
            Command::GetActiveBootSlot => {
                let slot = bootctl.getActiveBootSlot()?;
                info!("getActiveBootSlot result: {slot}");
            }
            Command::GetCurrentSlot => {
                let slot = bootctl.getCurrentSlot()?;
                info!("getCurrentSlot result: {slot}");
            }
            Command::GetNumberSlots => {
                let slots = bootctl.getNumberSlots()?;
                info!("getNumberSlots result: {slots}");
            }
            Command::GetSuffix { slot_index } => {
                let suffix = bootctl.getSuffix(*slot_index)?;
                info!("getSuffix {slot_index} result: {suffix}");
            }
            Command::IsSlotBootable { slot_index } => {
                let bootable = bootctl.isSlotBootable(*slot_index)?;
                info!("isSlotBootable {slot_index} result: {bootable}");
            }
            Command::IsSlotMarkedSuccessful { slot_index } => {
                let successful = bootctl.isSlotMarkedSuccessful(*slot_index)?;
                info!("isSlotMarkedSuccessful {slot_index} result: {successful}");
            }
            Command::SetSlotAsUnbootable { slot_index } => {
                bootctl.setSlotAsUnbootable(*slot_index)?;
                debug!("setSlotAsUnbootable {slot_index} complete");
            }
            Command::SetActiveBootSlot { slot_index } => {
                bootctl.setActiveBootSlot(*slot_index)?;
                debug!("setActiveBootSlot {slot_index} complete");
            }
            Command::MarkBootSuccessful => {
                bootctl.markBootSuccessful()?;
                debug!("markBootSuccessful complete");
            }
            Command::GetSnapshotMergeStatus => {
                let status = bootctl.getSnapshotMergeStatus()?;
                info!("getSnapshotMergeStatus result: {status:?}");
            }
            Command::SetSnapshotMergeStatus { status } => {
                bootctl.setSnapshotMergeStatus(*status)?;
                debug!("setSnapshotMergeStatus {status:?} complete");
            }
        };
        Ok(())
    }
}

fn parse_merge_status(arg: String) -> Result<MergeStatus, std::convert::Infallible> {
    Ok(match &*arg {
        "NONE" => MergeStatus::NONE,
        "UNKNOWN" => MergeStatus::UNKNOWN,
        "SNAPSHOTTED" => MergeStatus::SNAPSHOTTED,
        "MERGING" => MergeStatus::MERGING,
        "CANCELLED" => MergeStatus::CANCELLED,
        _ => unreachable!("Other values disallowed by the wrapping PossibleValuesParser"),
    })
}

fn setup_logger(level: log::LevelFilter) {
    // SAFETY: getppid is always successful; it has no safety preconditions.
    let ppid = unsafe { libc::getppid() };
    if ppid == 1 {
        // If init is calling us then it's during boot and we should log to kmsg
        let klog = KernelLog::with_level(level).expect("Couldn't create KernelLog.");
        log::set_boxed_logger(Box::new(klog)).expect("Couldn't set boxed logger.");
        log::set_max_level(level);
        return;
    }

    android_logger::init_once(
        android_logger::Config::default().with_tag("boot-ctl-cmd").with_max_level(level),
    );
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    setup_logger(cli.log_level);

    let bootctl =
        wait_for_interface::<dyn IBootControl>("android.hardware.boot.IBootControl/default")
            .expect("Couldn't connect to IBootControl.");

    match cli.cmd.call(&*bootctl) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("Call failed with error {}", e);
            ExitCode::FAILURE
        }
    }
}
