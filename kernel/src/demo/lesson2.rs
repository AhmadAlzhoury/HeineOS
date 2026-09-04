/*
 * Contains a demo for heap allocations.
 *
 * Author: Michael Schoetter, Heinrich Heine University Duesseldorf
 *         Fabian Ruhland, Heinrich Heine University Duesseldorf, 2026-01-14
 * License: GPLv3
 */

use alloc::boxed::Box;
use alloc::vec::Vec;
use crate::allocator;
use crate::device::key::Scancode;
use crate::device::keyboard::keyboard_buffer;
use crate::device::speaker;
use crate::device::speaker::SPEAKER;
use crate::device::terminal::terminal;

/// A simple heap demo, allocating and freeing memory on the heap.
/// The allocator state is dumped before and after each operation.
pub fn heap_demo() {
    struct DemoStruct {
        a: usize,
        b: usize,
    }

    fn wait_for_enter() {
        println!("\nPress Enter to continue...");

        loop {
            let key = keyboard_buffer().poll_key_press();
            if key.scancode() == Some(Scancode::Enter) {
                break;
            }
        }

        println!("");
    }

    terminal().lock().clear();
    println!("Heap Demo:\n");

    println!("Demo 1/4: Allocate structs using 'Box'");
    println!("--------------------------------------");

    allocator::global::dump_free_list();

    let s1 = Box::new(DemoStruct { a: 1, b: 2 });
    let s2 = Box::new(DemoStruct { a: 3, b: 4 });

    println!("s1 = {{ a: {}, b: {} }}", s1.a, s1.b);
    println!("s2 = {{ a: {}, b: {} }}", s2.a, s2.b);
    allocator::global::dump_free_list();
    wait_for_enter();

    println!("Demo 2/4: Free allocated structs");
    println!("---------------------------------");

    drop(s1);
    drop(s2);
    allocator::global::dump_free_list();
    wait_for_enter();

    println!("Demo 3/4: Allocate a Vec of three structs");
    println!("-----------------------------------------");

    let mut vec = Vec::new();
    vec.push(DemoStruct { a: 5, b: 6 });
    vec.push(DemoStruct { a: 7, b: 8 });
    vec.push(DemoStruct { a: 9, b: 0 });

    for (index, value) in vec.iter().enumerate() {
        println!("vec[{}] = {{ a: {}, b: {} }}", index, value.a, value.b);
    }
    allocator::global::dump_free_list();
    wait_for_enter();

    println!("Demo 4/4: Free allocated Vec");
    println!("----------------------------");

    drop(vec);
    allocator::global::dump_free_list();
    wait_for_enter();
}

/// A demo that plays songs via the PC speaker.
pub fn speaker_demo() {
    terminal().lock().clear();
    println!("PC Speaker Demo");
    println!("===============");
    println!("Playing a scale...");

    speaker::tetris();

    println!("Done.");
}