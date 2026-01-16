# Copyright (C) 2025 The Android Open Source Project
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

# This is the default set of packages to load the trusty memshare vm.
# It can be overridden by device-specific configuration.
PRODUCT_SOONG_NAMESPACES += \
    device/generic/trusty/custom_vm \

TRUSTY_MEMSHARE_VM_PRODUCT_PACKAGES ?= trusty_memshare_vm.elf.system \
	memshare_trusty_vm_launcher_system \
	trusty_memshare_vm_launcher.rc.system \
	trusty_memshare_vm_instance_id.system \
	trusty_memshare_vm_rpc_services.json.system \
	memshare_app.system \
	early_vms_memshare.xml \

PRODUCT_PACKAGES += $(TRUSTY_MEMSHARE_VM_PRODUCT_PACKAGES)
