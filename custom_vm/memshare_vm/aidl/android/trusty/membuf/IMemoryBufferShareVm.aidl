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
package android.trusty.membuf;

import android.trusty.membuf.ShareMemoryBufferResult;

/*
 * Interface used that provides a remote operation service, controlled via a list of commands.
 * The command list approach allows callers to execute a large set of operations on a single call,
 * hence reducing the IPC cost and allowing the remote service to potentially process commands concurrently.
 * It is expected that vendors inherit this generic interface to extend with their own commands, while
 * leveraging from the command list patterns, allowing to manage efficiently
 */
interface IMemoryBufferShareVm {
    /*
     * Share a memory buffer and retrieve a memory buffer context
     *
     * @param fd:
     *      fd handle wrapping a memory (dma) buffer.
     *
     * @param sizeBytes:
     *      size of the memory buffer.
     *
     * @return:
     *      SetMemoryBufferResult[] on success, which on success contains a memory buffer context
     *      to use to across other client interfaces.
     *
     * @throws:
     *      ServiceSpecificException based on <code>HalErrorCode</code> if any error occurs,
     *      in particular:
     *          - UNSUPPORTED if the requested operation is not supported by the server.
     *          - ALLOCATION_ERROR if the system runs out of memory while carring out the operation.
     */
    ShareMemoryBufferResult shareMemoryBuffer(long ipa, int sizeBytes);
}
