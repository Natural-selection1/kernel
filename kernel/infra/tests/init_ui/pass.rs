// Copyright (c) 2026 vivo Mobile Communication Co., Ltd.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//       http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![feature(allocator_api)]
use blueos_infra::init::{InPlaceInit, InPlaceWrite};
use pin_init::{init, pin_data, pin_init};
use std::{alloc::AllocError, marker::PhantomPinned, pin::Pin};
#[pin_data]
struct Value {
    #[pin]
    _pin: PhantomPinned,
}
pub fn check() -> Result<(), AllocError> {
    let _pinned: <Box<Value> as InPlaceInit<Value>>::PinnedSelf =
        Box::pin_init(pin_init!(Value {
            _pin: PhantomPinned
        }))?;
    let boxed = Box::init(42u32)?;
    let _moved = *boxed;
    let storage = Box::<u32>::try_new_uninit()?;
    let _initialized = storage.write_init(7u32).unwrap();
    Ok(())
}

pub fn opaque_positive() {
    use blueos_infra::opaque::Opaque;
    fn needs_send<T: Send>() {}
    needs_send::<Opaque<u32>>();
    let shell = Opaque::<u32>::uninit();
    let _pointer = shell.get();
}
