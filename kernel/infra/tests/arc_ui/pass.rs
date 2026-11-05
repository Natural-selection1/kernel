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
use blueos_infra::{
    arc::{Arc, UniqueArc},
    init::{InPlaceInit, InPlaceWrite},
};
use std::{cell::Cell, marker::PhantomPinned, pin::Pin};
fn send<T: Send>() {}
fn sync<T: Sync>() {}
fn unpin<T: Unpin>() {}
pub fn check() {
    send::<Arc<u32>>();
    sync::<Arc<u32>>();
    send::<UniqueArc<Cell<u32>>>();
    sync::<UniqueArc<u32>>();
    // MutexGuard is Sync but !Send: borrowing UniqueArc cannot transfer T.
    sync::<UniqueArc<std::sync::MutexGuard<'static, u32>>>();
    unpin::<Arc<PhantomPinned>>();
    unpin::<UniqueArc<PhantomPinned>>();
    let mut unique = UniqueArc::try_new(1u32).unwrap();
    *unique = 2;
    let shared: Arc<u32> = unique.into();
    let _: Pin<&u32> = shared.as_pin_ref();
    let pinned: Pin<UniqueArc<PhantomPinned>> = UniqueArc::try_new(PhantomPinned).unwrap().into();
    let _: Arc<PhantomPinned> = pinned.into();
}

pub fn initialization() {
    let shared: Arc<PhantomPinned> = Arc::pin_init(PhantomPinned).unwrap();
    let _: Pin<&PhantomPinned> = shared.as_pin_ref();
    let pinned: Pin<UniqueArc<PhantomPinned>> = UniqueArc::pin_init(PhantomPinned).unwrap();
    let _: Arc<PhantomPinned> = pinned.into();
    let storage = UniqueArc::<u32>::try_new_uninit().unwrap();
    let _: UniqueArc<u32> = storage.write_init(42u32).unwrap();
}
