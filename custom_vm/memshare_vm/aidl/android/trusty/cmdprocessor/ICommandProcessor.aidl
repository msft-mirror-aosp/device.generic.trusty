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
package android.trusty.cmdprocessor;

import android.trusty.cmdprocessor.MemoryBufferReference;

/*
 * Interface used that provides a remote operation service, controlled via a list of commands.
 * The command list approach allows callers to execute a large set of operations on a single call,
 * hence reducing the IPC cost and allowing the remote service to potentially process commands concurrently.
 * It is expected that vendors inherit this generic interface to extend with their own commands, while
 * leveraging from the command list patterns, allowing to manage efficiently
 */
interface ICommandProcessor {
    /*
     * Executes a command
     *
     * @param commandName: name of the command to execute
     *
     * @param buffers:
     *      Parameter containing 1 or more set of MemoryBufferReference to use as data for the command.
     *
     * @return:
     *   - ERR_BAD_TRANSACTION
     *
     * @throws:
     *      ServiceSpecificException based on <code>HalErrorCode</code> if any error occurs,
     *      in particular:
     *          - UNSUPPORTED if the requested operation is not supported by the server.
     *          - ALLOCATION_ERROR if the system runs out of memory while carring out the operation.
     */
    void processCommand(String commandName, in MemoryBufferReference[] memBufferRefs, in byte[] cmdDataIn, inout byte[] cmdDataOut);
}
