# Copyright (C) 2019 The Android Open Source Project
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#      http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
#

# The generic product target doesn't have any hardware-specific pieces.
TARGET_NO_BOOTLOADER := true
TARGET_NO_KERNEL := true
TARGET_ARCH := arm64
TARGET_ARCH_VARIANT := armv8-a
TARGET_CPU_VARIANT := generic
TARGET_CPU_ABI := arm64-v8a
TARGET_CPU_ABI2 :=
TARGET_BOOTLOADER_BOARD_NAME := trusty_$(TARGET_ARCH)

TARGET_2ND_ARCH := arm
TARGET_2ND_ARCH_VARIANT := armv8-a
TARGET_2ND_CPU_ABI := armeabi-v7a
TARGET_2ND_CPU_ABI2 := armeabi
TARGET_2ND_CPU_VARIANT := generic

BOARD_SEPOLICY_DIRS += device/generic/trusty/sepolicy


# We want goldfish build configuration information, but not the resulting
# QEMU images. QEMU_CUSTOMIZATIONS turns this on without building the images
# like BUILD_QEMU_IMAGES would imply.
QEMU_CUSTOMIZATIONS := true

# Include the ramdisk image into the target files because
# the prebuilts in the Trusty manifest need it there.
BOARD_IMG_USE_RAMDISK := true
BOARD_RAMDISK_USE_LZ4 := true
BOARD_USES_GENERIC_KERNEL_IMAGE := true

TARGET_KERNEL_USE ?= 6.6
TARGET_KERNEL_ARCH ?= $(TARGET_ARCH)
TARGET_KERNEL_PATH ?= kernel/prebuilts/$(TARGET_KERNEL_USE)/$(TARGET_KERNEL_ARCH)/kernel-$(TARGET_KERNEL_USE)
SYSTEM_DLKM_SRC ?= kernel/prebuilts/$(TARGET_KERNEL_USE)/$(TARGET_KERNEL_ARCH)

# Copy kernel image for use by emulator
PRODUCT_COPY_FILES += $(TARGET_KERNEL_PATH):kernel

# Distribute kernel image. Normally the kernel would be in boot.img,
# but because we do not use a boot.img we need to dist the kernel image itself.
ifneq ($(filter $(TARGET_PRODUCT), qemu_trusty_arm64),)
$(call dist-for-goals, dist_files, $(PRODUCT_OUT)/kernel)
endif

# The list of modules strictly/only required either to reach second stage
# init, OR for recovery. Do not use this list to workaround second stage
# issues.
VIRTUAL_DEVICE_MODULES_PATH ?= \
    kernel/prebuilts/common-modules/virtual-device/$(TARGET_KERNEL_USE)/$(subst _,-,$(TARGET_KERNEL_ARCH))
RAMDISK_VIRTUAL_DEVICE_MODULES := \
    failover.ko \
    net_failover.ko \
    virtio_blk.ko \
    virtio_console.ko \
    virtio_mmio.ko \
    virtio_net.ko \
    virtio_pci.ko \

# TODO(b/301606895): use kernel/prebuilts/common-modules/trusty when we have it
TRUSTY_MODULES_PATH ?= \
    kernel/prebuilts/common-modules/trusty/$(TARGET_KERNEL_USE)/$(subst _,-,$(TARGET_KERNEL_ARCH))
RAMDISK_TRUSTY_MODULES := \
    system_heap.ko \
    trusty-core.ko \
    trusty-ipc.ko \
    trusty-log.ko \
    trusty-test.ko \
    trusty-virtio.ko \

# Trusty modules should come after virtual device modules to preserve virtio
# device numbering and /dev devices names, which we rely on for the rpmb and
# test-runner virtio console ports.
BOARD_VENDOR_RAMDISK_KERNEL_MODULES := \
    $(wildcard $(patsubst %,$(VIRTUAL_DEVICE_MODULES_PATH)/%,$(RAMDISK_VIRTUAL_DEVICE_MODULES))) \
    $(wildcard $(patsubst %,$(SYSTEM_DLKM_SRC)/%,$(RAMDISK_VIRTUAL_DEVICE_MODULES))) \
    $(patsubst %,$(TRUSTY_MODULES_PATH)/%,$(RAMDISK_TRUSTY_MODULES)) \

# GKI >5.15 will have and require virtio_pci_legacy_dev.ko
BOARD_VENDOR_RAMDISK_KERNEL_MODULES += $(wildcard $(VIRTUAL_DEVICE_MODULES_PATH)/virtio_pci_legacy_dev.ko)
# GKI >5.10 will have and require virtio_pci_modern_dev.ko
BOARD_VENDOR_RAMDISK_KERNEL_MODULES += $(wildcard $(VIRTUAL_DEVICE_MODULES_PATH)/virtio_pci_modern_dev.ko)
# GKI >6.4 will have an required vmw_vsock_virtio_transport_common.ko and vsock.ko
BOARD_VENDOR_RAMDISK_KERNEL_MODULES += \
    $(wildcard $(VIRTUAL_DEVICE_MODULES_PATH)/vmw_vsock_virtio_transport_common.ko) \
    $(wildcard $(VIRTUAL_DEVICE_MODULES_PATH)/vsock.ko)

TARGET_USERIMAGES_USE_EXT4 := true
BOARD_SYSTEMIMAGE_PARTITION_SIZE := 536870912 # 512M
BOARD_USERDATAIMAGE_PARTITION_SIZE := 268435456 # 256M
TARGET_COPY_OUT_VENDOR := vendor
# ~100 MB vendor image. Please adjust system image / vendor image sizes
# when finalizing them.
BOARD_VENDORIMAGE_PARTITION_SIZE := 67108864 # 64M
BOARD_VENDORIMAGE_FILE_SYSTEM_TYPE := ext4
BOARD_FLASH_BLOCK_SIZE := 512
TARGET_USERIMAGES_SPARSE_EXT_DISABLED := true

BOARD_PROPERTY_OVERRIDES_SPLIT_ENABLED := true
BOARD_SEPOLICY_DIRS += build/target/board/generic/sepolicy

# Enable A/B update
TARGET_NO_RECOVERY := true

# Specify HALs
DEVICE_MANIFEST_FILE := device/generic/trusty/manifest.xml

# Enable full VNDK support
BOARD_VNDK_VERSION := current
