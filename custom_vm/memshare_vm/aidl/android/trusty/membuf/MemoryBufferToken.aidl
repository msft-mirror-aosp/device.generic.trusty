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

/*
 * Implementation defined structure that represents a memory buffer and its associated metadata.
 * The token is only valid as long as its IMemoryBufferContext parent's object is alive.
 */
@RustDerive(Clone=true)
parcelable MemoryBufferToken {
    /*
     * Opaque token used to perform operation on the buffer through different client interfaces;
     * This token shall only work when the service handing over the token is also the one implementing
     * the operation interface.
     */
    byte[] token;
}