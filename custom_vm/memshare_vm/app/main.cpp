/*
 * Copyright (C) 2025 The Android Open Source Project
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

#include <android-base/logging.h>
#include <android/trusty/cmdprocessor/ICommandProcessor.h>
#include <android/trusty/membuf/IMemoryBufferShare.h>
#include <linux/dma-heap.h>
#include <sys/auxv.h>
#include <sys/mman.h>

#include <binder/IBinder.h>
#include <binder/IServiceManager.h>

#define CMD_PROCESSOR_SERVICE_NAME "android.trusty.cmdprocessor.ICommandProcessor/memshare_vm"
#define BUFF_SHARE_SERVICE_NAME "android.trusty.membuf.IMemoryBufferShare/memshare_vm"
#define ACCESSOR_SERVICE_NAME "android.os.IAccessor/ICommandProcessor/memshare_vm"

using android::AccessorProvider;
using android::addAccessorProvider;
using android::IBinder;
using android::String16;
using android::trusty::cmdprocessor::ICommandProcessor;
using android::trusty::cmdprocessor::MemoryBufferReference;
using android::trusty::membuf::IMemoryBufferShare;
using android::trusty::membuf::MemoryBufferToken;
using android::trusty::membuf::ShareMemoryBufferResult;
using android::trusty::membuf::IMemoryBufferShare::HOST_BUFFER;
using android::trusty::membuf::IMemoryBufferShare::SECURE_DISPLAY_FRAME_BUFFER;

static std::weak_ptr<AccessorProvider> mAccessor;
// TODO(b/437876662): We should be able to remove the local caching of the accessor. Keeping local
//                    cache for now until bug is closed because connection seems to be faster.
static android::sp<ICommandProcessor> mAccessorCmdProcessor;

const char* shared_buffer_heap_device_name = "/dev/dma_heap/system";
const char* secure_buffer_heap_device_name = "/dev/dma_heap/example_heap";

#define BUFFER_SIZE 1024 * 1024

android::sp<IMemoryBufferShare> getMemShareBuffer() {
    auto binder =
        android::defaultServiceManager()->waitForService(String16(BUFF_SHARE_SERVICE_NAME));
    return interface_cast<IMemoryBufferShare>(binder);
}

android::sp<ICommandProcessor> getCmdProcessorAccessor() {
    if (!mAccessorCmdProcessor) {
        // TODO(b/437876662): mAccessor can be used to unregister the service once it is not needed
        //                    anymore. We need to implement this functionality for c++.
        mAccessor = addAccessorProvider({CMD_PROCESSOR_SERVICE_NAME},
                                        [&](const String16&) -> android::sp<IBinder> {
                                            return android::defaultServiceManager()->waitForService(
                                                String16(ACCESSOR_SERVICE_NAME));
                                        });

        mAccessorCmdProcessor =
            android::waitForService<ICommandProcessor>(String16(CMD_PROCESSOR_SERVICE_NAME));
        if (mAccessorCmdProcessor == nullptr) {
            ALOGE("couldn't get ICommandProcessor service\n");
        }
    }
    return mAccessorCmdProcessor;
}

static inline bool align_overflow(size_t size, size_t alignment, size_t* aligned) {
    if (size % alignment == 0) {
        *aligned = size;
        return false;
    }
    size_t temp = 0;
    bool overflow = __builtin_add_overflow(size / alignment, 1, &temp);
    overflow |= __builtin_mul_overflow(temp, alignment, aligned);
    return overflow;
}

static int allocate_buffers(size_t size, const char* device_name) {
    int dma_heap_fd = open(device_name, O_RDONLY | O_CLOEXEC);
    if (dma_heap_fd < 0) {
        LOG(ERROR) << "Cannot open " << device_name;
        return -1;
    }
    size_t aligned = 0;
    if (align_overflow(size, getauxval(AT_PAGESZ), &aligned)) {
        LOG(ERROR) << "Rounding up buffer size oveflowed";
        return -1;
    }
    struct dma_heap_allocation_data allocation_request = {
        .len = aligned,
        .fd_flags = O_RDWR | O_CLOEXEC,
    };
    int rc = ioctl(dma_heap_fd, DMA_HEAP_IOCTL_ALLOC, &allocation_request);
    if (rc < 0) {
        LOG(ERROR) << "Buffer allocation request failed  " << rc;
        return -1;
    }
    int fd = allocation_request.fd;
    if (fd < 0) {
        LOG(ERROR) << "Allocation request returned bad fd" << fd;
        return -1;
    }
    return fd;
}

int main(int /*argc*/, char* /*argv*/[]) {
    LOG(INFO) << "starting test application";

    // Retrieving accessor to communicate with ICommandProcessor implementation directly
    std::cout << "Starting test application" << std::endl;
    auto cmdProcessor = getCmdProcessorAccessor();
    if (cmdProcessor == nullptr) {
        LOG(ERROR) << "Couldn't get ICommandProcessor service";
        std::cout << "Couldn't get ICommandProcessor service" << std::endl;
        return -1;
    }
    std::vector<MemoryBufferReference> buffRef;
    std::vector<uint8_t> cmdIn;
    std::vector<uint8_t> cmdOut;
    auto ret = cmdProcessor->processCommand(android::String16("ping"), buffRef, cmdIn, &cmdOut);
    std::cout << "processCommand ping returned " << ret << std::endl;

    // Retrieving IMemoryBufferShare service implemented on VM launcher
    auto memSharing = getMemShareBuffer();
    if (memSharing == nullptr) {
        LOG(ERROR) << "Couldn't get getMemShareBuffer service";
        std::cout << "Couldn't get getMemShareBuffer service" << std::endl;
        return -1;
    }

    auto fd = allocate_buffers(BUFFER_SIZE, shared_buffer_heap_device_name);
    if (fd < 0) {
        LOG(ERROR) << "couldn't allocate buffer";
        std::cout << "couldn't allocate buffer" << std::endl;
        return -1;
    }
    uint8_t data[5] = {44, 2, 55, 87, 22};
    auto fd_copy = dup(fd);
    void* mapped_area = mmap(NULL, 4096, PROT_READ | PROT_WRITE, MAP_SHARED, fd_copy, 0);
    if (mapped_area == MAP_FAILED) {
        LOG(ERROR) << "mmap failed";
        std::cout << "mmap failed" << std::endl;
        return -1;
    } else {
        memcpy(mapped_area, data, 5);
    }
    android::base::unique_fd ufd(fd);
    android::os::ParcelFileDescriptor pfd(std::move(ufd));

    // Don't drop the ShareMemoryBufferResult object even after getting the token, that
    // could trigger a cleanup of the resources on the TA
    ShareMemoryBufferResult aidl_return;
    auto res = memSharing->shareMemoryBuffer(pfd, BUFFER_SIZE, HOST_BUFFER, &aidl_return);
    if (!res.isOk()) {
        LOG(ERROR) << "Couldn't share buffer";
        std::cout << "Couldn't share buffer" << std::endl;
        return -1;
    }
    MemoryBufferToken mem_token;

    res = aidl_return.context->getPortableToken(&mem_token);
    if (!res.isOk()) {
        LOG(ERROR) << "Couldn't get portable token";
        std::cout << "Couldn't get portable token" << std::endl;
        return -1;
    }
    MemoryBufferReference ref;
    ref.token = std::move(mem_token);
    ref.startOffset = 0;
    ref.sizeBytes = 4096;
    buffRef.push_back(std::move(ref));
    ret = cmdProcessor->processCommand(android::String16("print"), buffRef, cmdIn, &cmdOut);

    uint8_t* mapped_data = static_cast<uint8_t*>(mapped_area);
    std::cout << "about to compare" << std::endl;
    if (mapped_data[0] == 44) {
        std::cout << "comparison succeeded" << std::endl;
    } else {
        std::cout << "comparison failed: found " << std::hex
                  << static_cast<unsigned int>(mapped_data[0]) << std::endl;
    }

    ret = cmdProcessor->processCommand(android::String16("increment"), buffRef, cmdIn, &cmdOut);
    std::cout << "processCommand increment returned " << ret << std::endl;

    std::cout << "about to compare" << std::endl;
    if (mapped_data[0] == 45) {
        std::cout << "comparison succeeded" << std::endl;
    } else {
        std::cout << "comparison failed: found " << std::hex
                  << static_cast<unsigned int>(mapped_data[0]) << std::endl;
    }

    cmdIn.push_back(3);
    ret = cmdProcessor->processCommand(android::String16("print"), buffRef, cmdIn, &cmdOut);
    std::cout << "processCommand print returned " << ret << std::endl;

    std::cout << "about to compare" << std::endl;
    if (mapped_data[0] == 45) {
        std::cout << "comparison succeeded" << std::endl;
    } else {
        std::cout << "comparison failed: found " << std::hex
                  << static_cast<unsigned int>(mapped_data[0]) << std::endl;
    }

    int32_t release_result;
    res = aidl_return.context->releaseMemoryBufferContext(&release_result);
    std::cout << "releaseMemoryBufferContext release returned " << ret << std::endl;

    std::cout << "about to compare" << std::endl;
    if (mapped_data[0] == 45) {
        std::cout << "comparison succeeded" << std::endl;
    } else {
        std::cout << "comparison failed: found " << std::hex
                  << static_cast<unsigned int>(mapped_data[0]) << std::endl;
    }

    /*auto secure_fd = allocate_buffers(4096, secure_buffer_heap_device_name);
    if (secure_fd < 0) {
        LOG(ERROR) << "couldn't allocate buffer";
        std::cout << "couldn't allocate buffer" << std::endl;
        //return -1;
    } else {
        std::cout << "allocated secure buffer!!!!" << std::endl;
    }

    ShareMemoryBufferResult secure_buffer_context;
    res = memSharing->shareMemoryBuffer(pfd, BUFFER_SIZE, SECURE_DISPLAY_FRAME_BUFFER,
    &secure_buffer_context); if (!res.isOk()) { LOG(ERROR) << "Couldn't share secure buffer";
        std::cout << "Couldn't share secure buffer" << std::endl;
        return -1;
    }

    MemoryBufferToken sec_mem_token;
    res = secure_buffer_context.context->getPortableToken(&sec_mem_token);
    if (!res.isOk()) {
        LOG(ERROR) << "Couldn't get secure memory portable token";
        std::cout << "Couldn't get secure memory portable token" << std::endl;
        return -1;
    }

    MemoryBufferReference sec_ref;
    sec_ref.token = std::move(sec_mem_token);
    sec_ref.startOffset = 0;
    sec_ref.sizeBytes = 4096;
    std::vector<MemoryBufferReference> secBuffRef;
    secBuffRef.push_back(std::move(sec_ref));
    //ret = cmdProcessor->processCommand(android::String16("print"), secBuffRef, cmdIn, &cmdOut);
    //std::cout << "processCommand print for secure buffer returned " << ret << std::endl;

    res = secure_buffer_context.context->releaseMemoryBufferContext(&release_result);
    std::cout << "releaseMemoryBufferContext for secure buffer returned " << ret << std::endl;*/

    LOG(INFO) << "test application finished execution";
    std::cout << "test application finished execution" << std::endl;
    return 0;
}
