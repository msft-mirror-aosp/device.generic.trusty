/*
 * Copyright 2025 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#define LOG_TAG "boot-ctl-mock"

#include <array>
#include <cstddef>
#include <ios>
#include <limits>
#include <optional>
#include <span>
#include <string>
#include <string_view>

#include <aidl/android/hardware/boot/BnBootControl.h>
#include <aidl/android/hardware/boot/IBootControl.h>
#include <aidl/android/hardware/boot/MergeStatus.h>
#include <android-base/file.h>
#include <android-base/logging.h>
#include <android-base/parseint.h>
#include <android-base/properties.h>
#include <android-base/unique_fd.h>
#include <android/binder_auto_utils.h>
#include <android/binder_manager.h>
#include <android/binder_process.h>
#include <fcntl.h>
#include <strings.h>
#include <sys/types.h>

using ::aidl::android::hardware::boot::BnBootControl;
using ::aidl::android::hardware::boot::IBootControl;
using ::aidl::android::hardware::boot::MergeStatus;
using ::ndk::ScopedAStatus;

#define BOOT_CONTROL_CONFIG_DIR "/metadata/boot_control_mock"

constexpr size_t kBootSlots = 2;
constexpr std::array<std::string_view, kBootSlots> kSlotSuffixes = {"_a", "_b"};

struct BootSlotData {
    bool bootable;
    bool successful;
};

static std::optional<size_t> GetSlotProperty(std::span<const BootSlotData> slots,
                                             const std::string& prop_name) {
    std::string val = android::base::GetProperty(prop_name, "");
    if (val.empty()) {
        return std::nullopt;
    }

    size_t idx;
    bool success = android::base::ParseUint<size_t>(val, &idx, kBootSlots - 1);
    if (!success) {
        LOG(ERROR) << "Unable to parse value \"" << val << "\" of property " << prop_name
                   << " as size_t or parsed value was not in [0.." << kBootSlots
                   << " ): " << strerror(errno);
        return std::nullopt;
    }

    const BootSlotData& slot = slots[idx];
    if (!slot.bootable) {
        LOG(ERROR) << "Property " << prop_name << " requested boot to slot" << kSlotSuffixes[idx]
                   << " (idx: " << idx << ") but slot is not marked bootable.";
        return std::nullopt;
    }

    return std::make_optional(idx);
}

struct BootControlConfig {
    size_t curr_slot;
    size_t next_slot;
    BootSlotData slots[kBootSlots];

    constexpr static char kBootConfigPath[] = BOOT_CONTROL_CONFIG_DIR "/config";

    ScopedAStatus Write() {
        auto fd = android::base::unique_fd(
            open(kBootConfigPath, O_WRONLY | O_CREAT | O_TRUNC | O_SYNC, S_IRUSR | S_IWUSR));
        if (!fd.ok()) {
            LOG(ERROR) << "Failed to open (errno: " << strerror(errno)
                       << ") to BootControlConfig file for writing: " << kBootConfigPath;
            return ScopedAStatus::fromServiceSpecificErrorWithMessage(
                errno, "Opening BootControlConfig file for writing failed.");
        }

        bool success = android::base::WriteFully(fd, this, sizeof(*this));
        if (!success) {
            LOG(ERROR) << "Failed to write (errno: " << strerror(errno)
                       << ") to BootControlConfig file: " << kBootConfigPath;
            return ScopedAStatus::fromServiceSpecificErrorWithMessage(
                errno, "Writing BootControlConfig failed.");
        }

        return ScopedAStatus::ok();
    }

    static std::optional<BootControlConfig> Read() {
        auto fd = android::base::unique_fd(open(kBootConfigPath, O_RDONLY | O_SYNC, 0));
        if (!fd.ok()) {
            if (fd.get() == ENOENT) {
                LOG(INFO) << "BootControlConfig did not exist at " << kBootConfigPath;
            } else {
                LOG(WARNING) << "Could not open BootControlConfig for reading (" << strerror(errno)
                             << ") at " << kBootConfigPath;
            }
            return std::nullopt;
        }

        char buf[sizeof(BootControlConfig)];
        bool success = android::base::ReadFully(fd, &buf, sizeof(buf));
        if (!success) {
            LOG(WARNING) << "Could not read (errno: " << strerror(errno)
                         << ") BootControlConfig from " << kBootConfigPath;
            return std::nullopt;
        }
        auto result = std::make_optional<BootControlConfig>();
        memcpy(&*result, &buf, sizeof(BootControlConfig));

        if (result->curr_slot >= kBootSlots && result->next_slot >= kBootSlots) {
            LOG(ERROR)
                << "Read BootControlConfig from previous boot with incorrect BootData: {curr: "
                << result->curr_slot << " next: " << result->next_slot << " }. Must be within [0.."
                << kBootSlots << ").";
            return std::nullopt;
        }

        if (!result->slots[result->next_slot].bootable) {
            LOG(ERROR) << "BootControlConfig from previous boot said to boot into slot"
                       << kSlotSuffixes[result->next_slot] << "(idx: " << result->next_slot
                       << ") which is marked unbootable.";
            return std::nullopt;
        }
        return result;
    }

    static BootControlConfig ReadOrDefault() {
        auto config = BootControlConfig::Read();
        if (config.has_value()) {
            return *config;
        }

        LOG(INFO) << "BootControlConfig missing or corrupted at" << kBootConfigPath
                  << "; Using default.";
        return BootControlConfig{.curr_slot = 0,
                                 .next_slot = 0,
                                 .slots = {
                                     BootSlotData{.bootable = true, .successful = true},
                                     BootSlotData{.bootable = true, .successful = true},
                                 }};
    }

    bool UpdateByProperty(BootControlConfig last, bool last_boot_failed) {
        auto property_update =
            GetSlotProperty(slots, "ro.boot.vendor.mock_boot_ctrl.update_to_slot");
        if (!property_update.has_value()) {
            return false;
        }

        if (last_boot_failed && *property_update == last.next_slot) {
            LOG(ERROR) << "Can't update to slot" << kSlotSuffixes[*property_update]
                       << " (idx: " << *property_update
                       << ") because we're rolling back to it. Updating would leave us with no "
                          "successful slot to roll back to.";
            return false;
        }

        size_t rollback_slot;
        if (last_boot_failed) {
            /* Use the same rollback slot as the boot that just failed. */
            rollback_slot = last.next_slot;
        } else if (*property_update != last.curr_slot) {
            /* Last boot succeeded; use it as the rollback slot. */
            rollback_slot = last.curr_slot;
        } else {
            /* We're updating the same slot that just succeeded; find a new rollback slot. */
            bool found = false;
            for (size_t i = kBootSlots - 1; i > 0; --i) {
                size_t idx = (*property_update + i) % kBootSlots;
                if (slots[idx].successful) {
                    rollback_slot = idx;
                    found = true;
                    break;
                }
            }
            if (!found) {
                LOG(ERROR)
                    << "Can't update to slot" << kSlotSuffixes[*property_update]
                    << " (idx: " << *property_update
                    << ") because it's the last successful slot. Updating would leave us with no "
                       "successful slot to roll back to.";
                return false;
            }
        }

        LOG(DEBUG) << "Updating to slot" << kSlotSuffixes[*property_update]
                   << " (idx: " << *property_update
                   << ") as requested by property. If update fails, roll back to slot"
                   << kSlotSuffixes[rollback_slot] << " (idx: " << rollback_slot << ").";
        curr_slot = *property_update;
        next_slot = rollback_slot;
        slots[curr_slot].successful = false;
        return true;
    }

    static BootControlConfig FromLastBoot(BootControlConfig last) {
        BootControlConfig curr = last;

        bool last_boot_failed = !last.slots[last.curr_slot].successful;
        if (last_boot_failed) {
            /*
             * Make sure we don't try to boot to this broken slot until Android fixes it and marks
             * it active again.
             */
            LOG(DEBUG) << "Previous boot (slot" << kSlotSuffixes[last.curr_slot]
                       << ", idx: " << last.curr_slot << ") failed. Marking unbootable.";
            curr.slots[last.curr_slot].bootable = false;
        }

        if (curr.UpdateByProperty(last, last_boot_failed)) {
            return curr;
        }

        /* Boot to the slot we set to active (i.e. "next_slot") during the last boot. */
        curr.curr_slot = last.next_slot;
        /*
         * Until/unless setActiveBootSlot is called, the next_slot should be the last slot we
         * booted successfully. Normally, that's the same slot that the previous boot used,
         * unless the last failed, in which case it's the slot we rolled back to.
         */
        curr.next_slot = last_boot_failed ? last.next_slot : last.curr_slot;
        return curr;
    }
};

class BootControl final : public BnBootControl {
  public:
    BootControl(BootControlConfig cfg) : cfg_(cfg) {}

    ScopedAStatus getActiveBootSlot(int32_t* _aidl_return) override {
        *_aidl_return = cfg_.next_slot;
        LOG(DEBUG) << "getActiveBootSlot result: " << *_aidl_return;
        return ScopedAStatus::ok();
    }
    ScopedAStatus getCurrentSlot(int32_t* _aidl_return) override {
        *_aidl_return = cfg_.curr_slot;
        LOG(DEBUG) << "getCurrentSlot result: " << *_aidl_return;
        return ScopedAStatus::ok();
    }
    ScopedAStatus getNumberSlots(int32_t* _aidl_return) override {
        *_aidl_return = kBootSlots;
        LOG(DEBUG) << "getNumberSlots result: " << *_aidl_return;
        return ScopedAStatus::ok();
    }
    ScopedAStatus getSuffix(int32_t in_slot, std::string* _aidl_return) override {
        if (!slotValid(in_slot)) {
            LOG(ERROR) << "getSuffix slot: " << in_slot << " result: no such slot";
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        *_aidl_return = kSlotSuffixes[static_cast<size_t>(in_slot)];
        LOG(DEBUG) << "getSuffix slot: " << in_slot << " result: " << *_aidl_return;
        return ScopedAStatus::ok();
    }
    ScopedAStatus isSlotBootable(int32_t in_slot, bool* _aidl_return) override {
        const BootSlotData* slot = getSlot(in_slot);
        if (slot == nullptr) {
            LOG(ERROR) << "isSlotBootable slot: " << in_slot << " result: no such slot";
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        *_aidl_return = slot->bootable;
        LOG(DEBUG) << "isSlotBootable slot: " << in_slot
                   << " result: " << (*_aidl_return ? "bootable" : "not bootable");
        return ScopedAStatus::ok();
    }
    ScopedAStatus isSlotMarkedSuccessful(int32_t in_slot, bool* _aidl_return) override {
        const BootSlotData* slot = getSlot(in_slot);
        if (slot == nullptr) {
            LOG(ERROR) << "isSlotMarkedSuccessful slot: " << in_slot << " result: no such slot";
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        *_aidl_return = slot->successful;
        LOG(DEBUG) << "isSlotMarkedSuccessful slot: " << in_slot
                   << " result: " << (*_aidl_return ? "successful" : "not successful");
        return ScopedAStatus::ok();
    }

    ScopedAStatus setSlotAsUnbootable(int32_t in_slot) override {
        BootSlotData* slot = getSlot(in_slot);
        if (slot == nullptr) {
            LOG(ERROR) << "setSlotAsUnbootable slot: " << in_slot << " result: no such slot";
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        if (static_cast<size_t>(in_slot) == cfg_.curr_slot) {
            LOG(ERROR) << "setSlotAsUnbootable: slot " << in_slot << " is currently running";
            /* Can't write to a slot's boot partition while it's the one running. */
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        BootSlotData* curr = getSlot(cfg_.curr_slot);
        assert(curr != nullptr);
        if (!curr->successful) {
            LOG(ERROR) << "setSlotAsUnbootable: current slot (" << cfg_.curr_slot
                       << ") is not yet marked successful";
            /* Don't write until we know the current slot works so we can roll back if needed. */
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        /* We're about to write an update to this slot; can't boot to it until writing is done. */
        slot->bootable = false;
        slot->successful = false;
        LOG(INFO) << "setSlotAsUnbootable slot: " << in_slot;
        return cfg_.Write();
    }
    ScopedAStatus setActiveBootSlot(int32_t in_slot) override {
        BootSlotData* slot = getSlot(in_slot);
        if (slot == nullptr) {
            LOG(ERROR) << "setActiveBootSlot slot: " << in_slot << " result: no such slot";
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        if (static_cast<size_t>(in_slot) == cfg_.curr_slot) {
            LOG(ERROR) << "setActiveBootSlot: slot " << in_slot << " is currently running";
            /* We're already running this slot; we can't update to it. */
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        BootSlotData* curr = getSlot(cfg_.curr_slot);
        assert(curr != nullptr);
        if (!curr->successful) {
            LOG(ERROR) << "setActiveBootSlot: current slot (" << cfg_.curr_slot
                       << ") is not yet marked successful";
            /* Don't update until we know the current slot works so we can roll back if needed. */
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        /* Done writing an update to this slot; mark it bootable and try booting to it next time. */
        size_t previous_active = cfg_.next_slot;
        cfg_.next_slot = in_slot;
        slot->successful = false;
        slot->bootable = true;
        LOG(INFO) << "setActiveBootSlot slot: " << in_slot
                  << " (previous active: " << previous_active << " current: " << cfg_.curr_slot
                  << ")";

        return cfg_.Write();
    }
    ScopedAStatus markBootSuccessful() override {
        BootSlotData* slot = getSlot(cfg_.curr_slot);
        assert(slot != nullptr);

        /* The update was successful; keep booting into this slot in the future. */
        slot->successful = true;
        cfg_.next_slot = cfg_.curr_slot;
        LOG(INFO) << "markBootSuccessful slot: " << cfg_.curr_slot;

        return cfg_.Write();
    }

    ScopedAStatus getSnapshotMergeStatus(MergeStatus* _aidl_return) override {
        *_aidl_return = MergeStatus::NONE;
        LOG(DEBUG) << "getSuffix slot: " << cfg_.curr_slot
                   << " result: " << toString(*_aidl_return);
        return ScopedAStatus::ok();
    }
    ScopedAStatus setSnapshotMergeStatus(MergeStatus in_status) override {
        /*
         * This mock isn't communicating with the bootloader, so we can't uphold any of the
         * guarantees the non-NONE merge status make. The OS shouldn't try to set anything other
         * values because our device doesn't build with Virtual A/B enabled, but return an error in
         * case it tries.
         */
        if (in_status != MergeStatus::NONE) {
            LOG(ERROR) << "setSnapshotMergeStatus: " << toString(in_status);
            return ScopedAStatus::fromStatus(STATUS_INVALID_OPERATION);
        }

        LOG(INFO) << "setSnapshotMergeStatus: " << toString(in_status);
        return ScopedAStatus::ok();
    }

  private:
    BootControlConfig cfg_;

    bool slotValid(int32_t slot) const {
        return slot >= 0 && slot < static_cast<int32_t>(kBootSlots);
    }
    const BootSlotData* getSlot(int32_t slot) const {
        if (!slotValid(slot)) {
            return nullptr;
        }
        return &cfg_.slots[slot];
    }
    BootSlotData* getSlot(int32_t slot) {
        if (!slotValid(slot)) {
            return nullptr;
        }
        return &cfg_.slots[slot];
    }
};

int main(int, char* argv[]) {
    android::base::InitLogging(argv, android::base::KernelLogger);
    android::base::SetMinimumLogSeverity(android::base::DEBUG);

    auto prev_cfg = BootControlConfig::ReadOrDefault();
    auto cfg = BootControlConfig::FromLastBoot(prev_cfg);

    ScopedAStatus write_status = cfg.Write();
    if (!write_status.isOk()) {
        LOG(ERROR) << "Could not write BootControlConfig during startup: " << write_status;
    }
    LOG(DEBUG) << std::boolalpha
               << "Finished loading configs: \n\t read: { current: " << prev_cfg.curr_slot
               << " next: " << prev_cfg.next_slot << " }\n\tusing: { current: " << cfg.curr_slot
               << " next: " << cfg.next_slot << " }\n\t\tslot" << kSlotSuffixes[0]
               << ": { bootable: " << cfg.slots[0].bootable
               << " successful: " << cfg.slots[0].successful << " }\n\t\tslot" << kSlotSuffixes[1]
               << ": { bootable: " << cfg.slots[1].bootable
               << " successful: " << cfg.slots[1].successful << "}";

    ABinderProcess_setThreadPoolMaxThreadCount(0);
    std::shared_ptr<IBootControl> service = ndk::SharedRefBase::make<BootControl>(cfg);

    const std::string instance = std::string(BootControl::descriptor) + "/default";
    auto status = AServiceManager_addService(service->asBinder().get(), instance.c_str());
    CHECK_EQ(status, STATUS_OK) << "Failed to add service " << instance << " " << status;
    LOG(INFO) << "IBootControl AIDL service running...";

    ABinderProcess_joinThreadPool();
    return EXIT_FAILURE;
}
