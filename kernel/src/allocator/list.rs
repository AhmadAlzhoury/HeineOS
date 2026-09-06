/*
 * A heap allocator that uses a linked list to manage free memory blocks.
 * It allows for dynamic memory allocation and deallocation.
 *
 * Author: Philipp Oppermann, https://os.phil-opp.com/allocator-designs/
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-13
 */

use alloc::alloc::{GlobalAlloc, Layout};
use crate::allocator::global::{align_up, Locked};

/// Header of a free block in the list allocator.
struct ListNode {
    /// Size of the memory block
    size: usize,

    /// &'static mut type semantically describes an owned object behind a pointer.
    /// Basically, it’s a Box without a destructor that frees the object at the end of the scope.
    /// Its lifetime is static, meaning it will live for the entire duration of the program.
    /// Of course, this is not true in reality, as we might delete the list node at some point.
    /// But the compiler does not know this.
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    /// Create a new ListNode with the given size and no next node.
    const fn new(size: usize) -> Self {
        ListNode { size, next: None }
    }

    /// Get the start address of the memory block.
    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }

    /// Get the end address of the memory block.
    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

/// Aggregate statistics about the state of the heap.
///
/// The values are a snapshot taken while the allocator was locked.
/// They describe the kernel heap only and say nothing about the physical memory of the system.
#[derive(Copy, Clone, Debug)]
pub struct HeapStats {
    /// Total size of the heap in bytes.
    pub total: usize,
    /// Sum of the sizes of all blocks in the free list.
    pub free: usize,
    /// Bytes currently handed out to allocations (`total - free`).
    /// This includes padding added for alignment and for the allocator metadata.
    pub used: usize,
    /// Number of blocks in the free list. A high number indicates fragmentation.
    pub free_blocks: usize,
    /// Size of the largest single free block, i.e. the largest possible allocation.
    pub largest_free_block: usize,
}

/// A linked list allocator that uses a free list to manage memory.
pub struct LinkedListAllocator {
    head: ListNode,
    heap_start: usize,
    heap_end: usize,
}

impl LinkedListAllocator {
    /// Create a new empty linked list allocator.
    pub const fn new() -> LinkedListAllocator {
        LinkedListAllocator {
            head: ListNode::new(0),
            heap_start: 0,
            heap_end: 0,
        }
    }

    /// Initialize the allocator with the heap bounds given in the constructor.
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_start = heap_start;
        self.heap_end = heap_start
            .checked_add(heap_size)
            .expect("Heap end address overflowed");
        self.head.next = None;

        unsafe {
            self.add_free_block(heap_start, heap_size);
        }
    }

    /// Adds the given free memory block in address order and merges adjacent blocks.
    unsafe fn add_free_block(&mut self, addr: usize, size: usize) {
        assert_eq!(
            align_up(addr, align_of::<ListNode>()),
            addr,
            "Free block address is not aligned"
        );
        assert!(
            size >= size_of::<ListNode>(),
            "Free block is too small for allocator metadata"
        );

        let block_end = addr
            .checked_add(size)
            .expect("Free block end address overflowed");
        assert!(
            addr >= self.heap_start && block_end <= self.heap_end,
            "Free block lies outside the heap"
        );

        let mut current = &mut self.head;
        while let Some(ref next) = current.next {
            if next.start_addr() >= addr {
                break;
            }
            current = current.next.as_mut().unwrap();
        }

        if current.size > 0 {
            assert!(
                current.end_addr() <= addr,
                "Free block overlaps its predecessor"
            );
        }
        if let Some(ref next) = current.next {
            assert!(
                block_end <= next.start_addr(),
                "Free block overlaps its successor"
            );
        }

        let node_ptr = addr as *mut ListNode;
        unsafe {
            node_ptr.write(ListNode::new(size));
            let node = &mut *node_ptr;
            node.next = current.next.take();
            current.next = Some(node);
        }

        let inserted = current.next.as_mut().unwrap();
        let merge_successor = inserted
            .next
            .as_ref()
            .is_some_and(|next| inserted.end_addr() == next.start_addr());

        if merge_successor {
            let successor = inserted.next.take().unwrap();
            inserted.size += successor.size;
            inserted.next = successor.next.take();
        }

        if current.size > 0 && current.end_addr() == addr {
            let inserted = current.next.take().unwrap();
            current.size += inserted.size;
            current.next = inserted.next.take();
        }
    }

    /// Search a free block with the given size and alignment and remove it from the list.
    fn find_free_block(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        let mut current = &mut self.head;

        while let Some(ref mut block) = current.next {
            if let Ok(alloc_start) = Self::check_block_for_alloc(block, size, align) {
                let block = current.next.take().unwrap();
                current.next = block.next.take();
                return Some((block, alloc_start));
            }

            current = current.next.as_mut().unwrap();
        }

        None
    }

    /// Check if the given block is large enough for an allocation with `size` and `align`.
    fn check_block_for_alloc(block: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let alloc_start = align_up(block.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;

        if alloc_end > block.end_addr() {
            return Err(());
        }

        let excess_size = block.end_addr() - alloc_end;
        if excess_size > 0 && excess_size < size_of::<ListNode>() {
            return Err(());
        }

        Ok(alloc_start)
    }

    /// Adjust the given layout so that the resulting allocated memory
    /// block is also capable of storing a `ListNode`.
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(align_of::<ListNode>())
            .expect("adjusting alignment failed")
            .pad_to_align();
        let size = layout.size().max(size_of::<ListNode>());

        (size, layout.align())
    }

    /// Collect aggregate statistics by walking the free list once.
    ///
    /// This is a read-only operation and does not allocate, so it can safely be
    /// called while the allocator lock is held.
    pub fn stats(&self) -> HeapStats {
        let total = self.heap_end.saturating_sub(self.heap_start);
        let mut free = 0;
        let mut free_blocks = 0;
        let mut largest_free_block = 0;

        let mut current = self.head.next.as_deref();
        while let Some(block) = current {
            free += block.size;
            free_blocks += 1;
            if block.size > largest_free_block {
                largest_free_block = block.size;
            }
            current = block.next.as_deref();
        }

        HeapStats {
            total,
            free,
            used: total.saturating_sub(free),
            free_blocks,
            largest_free_block,
        }
    }

    /// Dump the free list for debugging purposes.
    pub fn dump_free_list(&mut self) {
        println!("Free list for heap {:#x}-{:#x}:", self.heap_start, self.heap_end);

        let mut current = self.head.next.as_deref();
        let mut index = 0;
        let mut total_free = 0;

        while let Some(block) = current {
            println!(
                "  [{}] {:#x}-{:#x}, {} bytes",
                index,
                block.start_addr(),
                block.end_addr(),
                block.size
            );
            total_free += block.size;
            index += 1;
            current = block.next.as_deref();
        }

        println!("Free blocks: {}, total free: {} bytes", index, total_free);
    }

    /// Allocate memory of the given size and alignment.
    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let (size, align) = Self::size_align(layout);

        if let Some((block, alloc_start)) = self.find_free_block(size, align) {
            let alloc_end = alloc_start + size;
            let excess_size = block.end_addr() - alloc_end;

            if excess_size > 0 {
                unsafe {
                    self.add_free_block(alloc_end, excess_size);
                }
            }

            alloc_start as *mut u8
        } else {
            core::ptr::null_mut()
        }
    }

    /// Free the memory block at the given pointer with the given layout.
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, _) = LinkedListAllocator::size_align(layout);

        unsafe {
            self.add_free_block(ptr as usize, size)
        }
    }
}

// Trait required by the Rust runtime for heap allocations
unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe {
            self.lock().alloc(layout)
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe {
            self.lock().dealloc(ptr, layout);
        }
    }
}