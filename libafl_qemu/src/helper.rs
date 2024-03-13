use core::{fmt::Debug, ops::Range};
use std::{collections::HashSet, hash::BuildHasher};

use libafl::{executors::ExitKind, inputs::UsesInput, observers::ObserversTuple};
use libafl_bolts::tuples::{MatchFirstType, SplitBorrowExtractFirstType};
use libafl_qemu_sys::{GuestAddr, GuestPhysAddr};

use crate::{hooks::QemuHooks, Qemu, QemuEdgeCoverageHelper};

/// A helper for `libafl_qemu`.
// TODO remove 'static when specialization will be stable
pub trait QemuHelper<S>: 'static + Debug
where
    S: UsesInput,
{
    const HOOKS_DO_SIDE_EFFECTS: bool = true;

    fn init_hooks<QT>(&self, _hooks: &QemuHooks<QT, S>)
    where
        QT: QemuHelperTuple<S>,
    {
    }

    fn first_exec<QT>(&self, _hooks: &QemuHooks<QT, S>)
    where
        QT: QemuHelperTuple<S>,
    {
    }

    fn pre_exec(&mut self, _emulator: &Qemu, _input: &S::Input) {}

    fn post_exec<OT>(
        &mut self,
        _emulator: &Qemu,
        _input: &S::Input,
        _observers: &mut OT,
        _exit_kind: &mut ExitKind,
    ) where
        OT: ObserversTuple<S>,
    {
    }
}

pub trait QemuHelperTuple<S>: MatchFirstType + for<'a> SplitBorrowExtractFirstType<'a>
where
    S: UsesInput,
{
    const HOOKS_DO_SIDE_EFFECTS: bool;

    fn init_hooks_all<QT>(&self, hooks: &QemuHooks<QT, S>)
    where
        QT: QemuHelperTuple<S>;

    fn first_exec_all<QT>(&self, hooks: &QemuHooks<QT, S>)
    where
        QT: QemuHelperTuple<S>;

    fn pre_exec_all(&mut self, _emulator: &Qemu, input: &S::Input);

    fn post_exec_all<OT>(
        &mut self,
        _emulator: &Qemu,
        input: &S::Input,
        _observers: &mut OT,
        _exit_kind: &mut ExitKind,
    ) where
        OT: ObserversTuple<S>;
}

impl<S> QemuHelperTuple<S> for ()
where
    S: UsesInput,
{
    const HOOKS_DO_SIDE_EFFECTS: bool = false;

    fn init_hooks_all<QT>(&self, _hooks: &QemuHooks<QT, S>)
    where
        QT: QemuHelperTuple<S>,
    {
    }

    fn first_exec_all<QT>(&self, _hooks: &QemuHooks<QT, S>)
    where
        QT: QemuHelperTuple<S>,
    {
    }

    fn pre_exec_all(&mut self, _emulator: &Qemu, _input: &S::Input) {}

    fn post_exec_all<OT>(
        &mut self,
        _emulator: &Qemu,
        _input: &S::Input,
        _observers: &mut OT,
        _exit_kind: &mut ExitKind,
    ) where
        OT: ObserversTuple<S>,
    {
    }
}

impl<Head, Tail, S> QemuHelperTuple<S> for (Head, Tail)
where
    Head: QemuHelper<S>,
    Tail: QemuHelperTuple<S>,
    S: UsesInput,
{
    const HOOKS_DO_SIDE_EFFECTS: bool = Head::HOOKS_DO_SIDE_EFFECTS || Tail::HOOKS_DO_SIDE_EFFECTS;

    fn init_hooks_all<QT>(&self, hooks: &QemuHooks<QT, S>)
    where
        QT: QemuHelperTuple<S>,
    {
        self.0.init_hooks(hooks);
        self.1.init_hooks_all(hooks);
    }

    fn first_exec_all<QT>(&self, hooks: &QemuHooks<QT, S>)
    where
        QT: QemuHelperTuple<S>,
    {
        self.0.first_exec(hooks);
        self.1.first_exec_all(hooks);
    }

    fn pre_exec_all(&mut self, emulator: &Qemu, input: &S::Input) {
        self.0.pre_exec(emulator, input);
        self.1.pre_exec_all(emulator, input);
    }

    fn post_exec_all<OT>(
        &mut self,
        emulator: &Qemu,
        input: &S::Input,
        observers: &mut OT,
        exit_kind: &mut ExitKind,
    ) where
        OT: ObserversTuple<S>,
    {
        self.0.post_exec(emulator, input, observers, exit_kind);
        self.1.post_exec_all(emulator, input, observers, exit_kind);
    }
}

#[derive(Debug, Clone)]
pub enum QemuFilterList<T: IsFilter + Debug + Clone> {
    AllowList(T),
    DenyList(T),
    None,
}

impl<T> IsFilter for QemuFilterList<T>
where
    T: IsFilter + Clone,
{
    type FilterParameter = T::FilterParameter;

    fn allowed(&self, filter_parameter: Self::FilterParameter) -> bool {
        match self {
            QemuFilterList::AllowList(allow_list) => allow_list.allowed(filter_parameter),
            QemuFilterList::DenyList(deny_list) => !deny_list.allowed(filter_parameter),
            QemuFilterList::None => true,
        }
    }
}

pub type QemuInstrumentationPagingFilter = QemuFilterList<HashSet<GuestPhysAddr>>;

impl<H> IsFilter for HashSet<GuestPhysAddr, H>
where
    H: BuildHasher,
{
    type FilterParameter = Option<GuestPhysAddr>;

    fn allowed(&self, paging_id: Self::FilterParameter) -> bool {
        paging_id.is_some_and(|pid| self.contains(&pid))
    }
}

pub type QemuInstrumentationAddressRangeFilter = QemuFilterList<Vec<Range<GuestAddr>>>;

impl IsFilter for Vec<Range<GuestAddr>> {
    type FilterParameter = GuestAddr;

    fn allowed(&self, addr: Self::FilterParameter) -> bool {
        for rng in self {
            if rng.contains(&addr) {
                return true;
            }
        }
        false
    }
}

pub trait HasInstrumentationFilter<F>
where
    F: IsFilter,
{
    fn filter(&self) -> &F;

    fn filter_mut(&mut self) -> &mut F;

    fn update_filter(&mut self, filter: F, emu: &Qemu) {
        *self.filter_mut() = filter;
        emu.flush_jit();
    }
}

#[cfg(emulation_mode = "usermode")]
pub trait StdInstrumentationFilter:
    HasInstrumentationFilter<QemuInstrumentationAddressRangeFilter>
{
}

#[cfg(emulation_mode = "systemmode")]
pub trait StdInstrumentationFilter:
    HasInstrumentationFilter<QemuInstrumentationAddressRangeFilter>
    + HasInstrumentationFilter<QemuInstrumentationPagingFilter>
{
}

impl StdInstrumentationFilter for (QemuEdgeCoverageHelper, ()) {}

#[cfg(emulation_mode = "systemmode")]
impl HasInstrumentationFilter<QemuInstrumentationPagingFilter> for (QemuEdgeCoverageHelper, ()) {
    fn filter(&self) -> &QemuInstrumentationPagingFilter {
        self.0.filter()
    }

    fn filter_mut(&mut self) -> &mut QemuInstrumentationPagingFilter {
        self.0.filter_mut()
    }
}

impl HasInstrumentationFilter<QemuInstrumentationAddressRangeFilter>
    for (QemuEdgeCoverageHelper, ())
{
    fn filter(&self) -> &QemuInstrumentationAddressRangeFilter {
        self.0.filter()
    }

    fn filter_mut(&mut self) -> &mut QemuInstrumentationAddressRangeFilter {
        self.0.filter_mut()
    }
}

pub trait IsFilter: Debug {
    type FilterParameter;

    fn allowed(&self, filter_parameter: Self::FilterParameter) -> bool;
}

pub trait IsAddressFilter: IsFilter<FilterParameter = GuestAddr> {}

#[cfg(emulation_mode = "systemmode")]
pub trait IsPagingFilter: IsFilter<FilterParameter = Option<GuestPhysAddr>> {}

#[cfg(emulation_mode = "systemmode")]
impl IsPagingFilter for QemuInstrumentationPagingFilter {}

impl IsAddressFilter for QemuInstrumentationAddressRangeFilter {}

#[must_use]
pub fn hash_me(mut x: u64) -> u64 {
    x = (x.overflowing_shr(16).0 ^ x).overflowing_mul(0x45d9f3b).0;
    x = (x.overflowing_shr(16).0 ^ x).overflowing_mul(0x45d9f3b).0;
    x = (x.overflowing_shr(16).0 ^ x) ^ x;
    x
}
