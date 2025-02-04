#
# Copyright (C) 2018-2019 The Android Open Source Project
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

# This file contains the definitions needed for a _really_ minimal system
# image to be run under emulation under upstream QEMU (www.qemu.org), once
# it supports a few Android virtual devices. Note that this is _not_ the
# same as running under the Android emulator.

$(call inherit-product, $(SRC_TARGET_DIR)/product/default_art_config.mk)
$(call inherit-product, $(SRC_TARGET_DIR)/product/updatable_apex.mk)

$(call inherit-product, packages/modules/Virtualization/apex/product_packages.mk)

PRODUCT_SOONG_NAMESPACES += device/generic/goldfish

# select minimal set of services from build/make/target/product/base_system.mk
PRODUCT_PACKAGES += \
    aconfigd-system \
    adbd_system_api \
    aflags \
    com.android.adbd \
    com.android.virt \
    adbd_system_api \
    android.hardware.confirmationui@1.0-service.trusty \
    android.hidl.allocator@1.0-service \
    android.system.suspend-service \
    apexd \
    cgroups.json \
    com.android.art \
    com.android.i18n \
    com.android.os.statsd \
    com.android.runtime \
    com.android.sdkext \
    dhcpclient \
    etc_hosts \
    gatekeeperd \
    hwservicemanager \
    init_system \
    init_vendor \
    init.environ.rc \
    keymaster_soft_wrapped_attestation_keys.xml \
    keystore2 \
    libandroid_servers \
    libc.bootstrap \
    libdl.bootstrap \
    libdl_android.bootstrap \
    libm.bootstrap \
    linker \
    linker64 \
    logcat \
    logd \
    logwrapper \
    mediaserver \
    mdnsd \
    odsign \
    reboot \
    securedpud \
    servicemanager \
    sh \
    su \
    system-build.prop \
    toolbox \
    toybox \
    vdc \
    vndservicemanager \
    vold \
    sanitizer.libraries.txt \

# VINTF stuff for system and vendor (no product / odm / system_ext / etc.)
PRODUCT_PACKAGES += \
    system_compatibility_matrix.xml \
    system_manifest.xml \
    vendor_compatibility_matrix.xml \
    vendor_manifest.xml \

PRODUCT_USE_DYNAMIC_PARTITIONS := true
TARGET_COPY_OUT_SYSTEM_EXT := system/system_ext
BOARD_SYSTEM_EXTIMAGE_FILE_SYSTEM_TYPE :=
SYSTEM_EXT_PRIVATE_SEPOLICY_DIRS += device/generic/trusty/sepolicy/system_ext/private

# Creates metadata partition mount point under root for
# the devices with metadata partition
BOARD_USES_METADATA_PARTITION := true

# Devices that inherit from build/make/target/product/base.mk always have
# /system/system_ext/etc/vintf/manifest.xml generated. And build-time VINTF
# checks assume that. Since we don't inherit from base.mk, add the dependency
# here manually.
PRODUCT_PACKAGES += \
    system_ext_manifest.xml \

# Skip VINTF checks for kernel configs
PRODUCT_OTA_ENFORCE_VINTF_KERNEL_REQUIREMENTS := false

# Ensure boringssl NIAP check won't reboot us
PRODUCT_PACKAGES += \
    com.android.conscrypt \
    boringssl_self_test \

# SELinux packages are added as dependencies of the selinux_policy
# phony package.
PRODUCT_PACKAGES += \
    selinux_policy \

PRODUCT_HOST_PACKAGES += \
    adb \
    e2fsdroid \
    make_f2fs \
    mke2fs \
    sload_f2fs \
    toybox \

PRODUCT_PACKAGES += init.usb.rc init.usb.configfs.rc

PRODUCT_FULL_TREBLE_OVERRIDE := true

PRODUCT_AVF_MICRODROID_GUEST_GKI_VERSION := android16_612

PRODUCT_COPY_FILES += \
    device/generic/trusty/fstab.trusty:$(TARGET_COPY_OUT_RAMDISK)/fstab.qemu_trusty \
    device/generic/trusty/fstab.trusty:$(TARGET_COPY_OUT_VENDOR)/etc/fstab.qemu_trusty \
    device/generic/trusty/init.qemu_trusty.rc:$(TARGET_COPY_OUT_VENDOR)/etc/init/hw/init.qemu_trusty.rc \
    device/generic/trusty/ueventd.qemu_trusty.rc:$(TARGET_COPY_OUT_VENDOR)/etc/ueventd.rc \
    system/core/libprocessgroup/profiles/task_profiles.json:$(TARGET_COPY_OUT_VENDOR)/etc/task_profiles.json \

PRODUCT_COPY_FILES += \
    device/generic/goldfish/data/etc/config.ini:config.ini \
    device/generic/trusty/advancedFeatures.ini:advancedFeatures.ini \

# Set Vendor SPL to match platform
# needed for properly provisioning keymint (HAL info)
VENDOR_SECURITY_PATCH = $(PLATFORM_SECURITY_PATCH)

# for Trusty
KEYMINT_HAL_VENDOR_APEX_SELECT ?= true
ifeq ($(RELEASE_AVF_ENABLE_EARLY_VM),true)
  # TODO(b/390206831): update to secure mode once Trusted HAL on virtio-msg can be enabled
  # in nonsecure mode, the Trusty System VMs (e.g. Keymint) is standalone and
  # uses local Trusted HAL services (HWCrypto/SecureStorage).
  # in secure mode, the Trusty System VMs access the remote Trusted HAL services,
  # and integrate AuthMgrFE.
  # the TRUSTY_SYSTEM_VM make variable is used by the Trusty makefile to customise
  # the content of the VM according to the desired secure/nonsecure mode.
  TRUSTY_SYSTEM_VM ?= nonsecure

endif

$(call inherit-product, device/generic/trusty/apex/com.android.hardware.keymint/trusty-apex.mk)
$(call inherit-product, system/core/trusty/trusty-base.mk)
$(call inherit-product, system/core/trusty/trusty-storage.mk)
$(call inherit-product, system/core/trusty/trusty-test.mk)

# Test Utilities
PRODUCT_PACKAGES += \
    binderRpcToTrustyTest \
    tipc-test \
    trusty-coverage-controller \
    trusty-ut-ctrl \
    trusty_stats_test \
    VtsAidlKeyMintTargetTest \
    VtsHalConfirmationUIV1_0TargetTest \
    VtsHalGatekeeperTargetTest \
    VtsHalGatekeeperV1_0TargetTest \
    VtsHalKeymasterV3_0TargetTest \
    VtsHalKeymasterV4_0TargetTest \
    VtsHalRemotelyProvisionedComponentTargetTest \

PRODUCT_SYSTEM_DEFAULT_PROPERTIES += \
    ro.adb.secure=0 \
    ro.boot.vendor.apex.com.android.hardware.keymint=com.android.hardware.keymint.trusty_tee.cpp \
