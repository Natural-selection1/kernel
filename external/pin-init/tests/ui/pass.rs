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
use pin_init::{pin_data, pin_init, InPlaceInit, Wrapper};
use std::{cell::UnsafeCell, marker::PhantomPinned};
#[pin_data]
struct Value {
    #[pin]
    pin: PhantomPinned,
}
pub fn check() {
    let _value = Box::pin_init(UnsafeCell::pin_init(pin_init!(Value {
        pin: PhantomPinned
    })))
    .unwrap();
}
