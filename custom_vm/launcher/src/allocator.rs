// Copyright (C) 2026 The Android Open-Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Region allocator for memshare service.

use std::iter::once;
use std::ops::Range;

/// Simple gap-based region allocator.
pub struct GapAllocator {
    region: Range<u64>,
    allocs: Vec<Range<u64>>,
}

// GapAllocator algorithm follows what is used on external/trusty/lk/lib/region_alloc/src/lib.rs
impl GapAllocator {
    /// Create a new GapAllocator for the given region.
    pub fn new(region: Range<u64>) -> Self {
        Self { region, allocs: Vec::new() }
    }

    /// Allocate a region of the given size and alignment.
    pub fn alloc(&mut self, size: u64, align: u64) -> Option<u64> {
        if size == 0 {
            return None;
        }
        let gap_start_iter = once(self.region.start).chain(self.allocs.iter().map(|r| r.end));
        let gap_end_iter = self.allocs.iter().map(|r| r.start).chain(once(self.region.end));
        for (idx, (gap_start, gap_end)) in gap_start_iter.zip(gap_end_iter).enumerate() {
            let aligned_start = gap_start.next_multiple_of(align);
            if aligned_start >= gap_end || size > gap_end - aligned_start {
                continue;
            }
            self.allocs.insert(idx, aligned_start..aligned_start + size);
            return Some(aligned_start);
        }
        None
    }

    /// Deallocate the region starting at the given address.
    pub fn dealloc(&mut self, start: u64) -> bool {
        if let Ok(idx) = self.allocs.binary_search_by_key(&start, |r| r.start) {
            self.allocs.remove(idx);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gap_allocator_host_regions() {
        let region_start = 0x8000_0000 + 64 * 1024 * 1024; // 0x8400_0000
        let region_end = region_start + 2 * 64 * 1024 * 1024; // 0x8C00_0000
        let mut allocator = GapAllocator::new(region_start..region_end);
        let size = 64 * 1024 * 1024;
        let alignment = 4096;

        // Verify we can create the 2 host regions we had before as fixed vectors
        assert_eq!(allocator.alloc(size, alignment), Some(0x8400_0000));
        assert_eq!(allocator.alloc(size, alignment), Some(0x8800_0000));
        // No more space
        assert_eq!(allocator.alloc(size, alignment), None);

        // Verify dealloc works and we can re-allocate
        assert!(allocator.dealloc(0x8400_0000));
        assert_eq!(allocator.alloc(size, alignment), Some(0x8400_0000));
    }

    #[test]
    fn test_gap_allocator_secure_regions() {
        let region_start = 0x9000_0000;
        let region_end = region_start + 6 * 64 * 1024 * 1024; // 0xA800_0000
        let mut allocator = GapAllocator::new(region_start..region_end);
        let size = 64 * 1024 * 1024;
        let alignment = 4096;

        // Verify we can create the 6 secure regions we had before as fixed vectors
        assert_eq!(allocator.alloc(size, alignment), Some(0x9000_0000));
        assert_eq!(allocator.alloc(size, alignment), Some(0x9400_0000));
        assert_eq!(allocator.alloc(size, alignment), Some(0x9800_0000));
        assert_eq!(allocator.alloc(size, alignment), Some(0x9C00_0000));
        assert_eq!(allocator.alloc(size, alignment), Some(0xA000_0000));
        assert_eq!(allocator.alloc(size, alignment), Some(0xA400_0000));
        // No more space
        assert_eq!(allocator.alloc(size, alignment), None);
    }
}
