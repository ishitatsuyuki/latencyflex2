#![allow(dead_code)]

use std::mem::MaybeUninit;
use std::ops;

use spark::vk;

macro_rules! impl_bitmask {
    ($name:ident, $all_bits:literal) => {
        impl $name {
            pub fn empty() -> Self {
                Self(0)
            }
            pub fn all() -> Self {
                Self($all_bits)
            }
            pub fn is_empty(self) -> bool {
                self.0 == 0
            }
            pub fn is_all(self) -> bool {
                self.0 == $all_bits
            }
            pub fn intersects(self, other: Self) -> bool {
                (self.0 & other.0) != 0
            }
            pub fn contains(self, other: Self) -> bool {
                (self.0 & other.0) == other.0
            }
        }
        impl ops::BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }
        impl ops::BitOrAssign for $name {
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }
        impl ops::BitAnd for $name {
            type Output = Self;
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }
        impl ops::BitAndAssign for $name {
            fn bitand_assign(&mut self, rhs: Self) {
                self.0 &= rhs.0;
            }
        }
        impl ops::BitXor for $name {
            type Output = Self;
            fn bitxor(self, rhs: Self) -> Self {
                Self(self.0 ^ rhs.0)
            }
        }
        impl ops::BitXorAssign for $name {
            fn bitxor_assign(&mut self, rhs: Self) {
                self.0 ^= rhs.0;
            }
        }
    };
}

#[repr(transparent)]
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct VkLayerFunction(std::os::raw::c_uint);
impl VkLayerFunction {
    pub const VK_LAYER_LINK_INFO: Self = Self(0);
    pub const VK_LOADER_DATA_CALLBACK: Self = Self(1);
    pub const VK_LOADER_LAYER_CREATE_DEVICE_CALLBACK: Self = Self(2);
    pub const VK_LOADER_FEATURES: Self = Self(3);
}

pub type FnLayerCreateDevice = Option<
    unsafe extern "C" fn(
        instance: VkInstance,
        physical_device: VkPhysicalDevice,
        p_create_info: *const VkDeviceCreateInfo,
        p_allocator: *const VkAllocationCallbacks,
        p_device: *mut VkDevice,
        layer_gipa: vk::FnGetInstanceProcAddr,
        next_gdpa: *mut vk::FnGetDeviceProcAddr,
    ) -> VkResult,
>;
pub type FnLayerDestroyDevice = Option<
    unsafe extern "C" fn(
        physical_device: VkDevice,
        p_allocator: *const VkAllocationCallbacks,
        destroy_function: vk::FnDestroyDevice,
    ),
>;

pub type FnSetInstanceLoaderData = Option<
    unsafe extern "C" fn(instance: VkInstance, object: *mut std::os::raw::c_void) -> VkResult,
>;
pub type FnSetDeviceLoaderData =
    Option<unsafe extern "C" fn(device: VkDevice, object: *mut std::os::raw::c_void) -> VkResult>;

pub type FnGetPhysicalDeviceProcAddr = vk::FnGetInstanceProcAddr;

type VkInstance = vk::Instance;
type VkPhysicalDevice = vk::PhysicalDevice;
type VkDevice = vk::Device;
type VkStructureType = vk::StructureType;
type VkResult = vk::Result;
type VkDeviceCreateInfo = vk::DeviceCreateInfo;
type VkAllocationCallbacks = vk::AllocationCallbacks;

#[repr(transparent)]
#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Hash)]
pub struct VkLoaderFeatureFlags(vk::Flags);
impl_bitmask!(VkLoaderFeatureFlags, 0x1);
impl VkLoaderFeatureFlags {
    pub const PHYSICAL_DEVICE_SORTING: Self = Self(0x00000001);
}

#[repr(C)]
#[derive(Debug, Default, Copy, Clone)]
pub struct VkLayerInstanceCreateInfoUFieldLayerDeviceField {
    pub pfn_layer_create_device: FnLayerCreateDevice,
    pub pfn_layer_destroy_device: FnLayerDestroyDevice,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct VkLayerInstanceLink {
    pub p_next: *mut VkLayerInstanceLink,
    pub pfn_next_get_instance_proc_addr: vk::FnGetInstanceProcAddr,
    pub pfn_next_get_physical_device_proc_addr: FnGetPhysicalDeviceProcAddr,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct VkLayerDeviceLink {
    pub p_next: *mut VkLayerDeviceLink,
    pub pfn_next_get_instance_proc_addr: vk::FnGetInstanceProcAddr,
    pub pfn_next_get_device_proc_addr: vk::FnGetDeviceProcAddr,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union VkLayerInstanceCreateInfoUField {
    pub p_layer_info: *mut VkLayerInstanceLink,
    pub pfn_set_instance_loader_data: FnSetInstanceLoaderData,
    pub layer_device: VkLayerInstanceCreateInfoUFieldLayerDeviceField,
    pub loader_features: VkLoaderFeatureFlags,
}

impl Default for VkLayerInstanceCreateInfoUField {
    fn default() -> Self {
        let mut s = MaybeUninit::<Self>::uninit();
        unsafe {
            std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct VkLayerInstanceCreateInfo {
    /// A `VkStructureType` value identifying this struct. Must be
    /// `VK_STRUCTURE_TYPE_LOADER_INSTANCE_CREATE_INFO`.
    pub s_type: VkStructureType,
    /// Either `NULL` or a pointer to a structure extending this structure.
    pub p_next: *const std::os::raw::c_void,
    /// A [`VkLayerFunction`] value identifying the payload in the `u` field.
    pub function: VkLayerFunction,
    /// The actual payload.
    pub u: VkLayerInstanceCreateInfoUField,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union VkLayerDeviceCreateInfoUField {
    pub p_layer_info: *mut VkLayerDeviceLink,
    pub pfn_set_device_loader_data: FnSetDeviceLoaderData,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct VkLayerDeviceCreateInfo {
    pub s_type: VkStructureType,
    pub p_next: *const std::os::raw::c_void,
    pub function: VkLayerFunction,
    pub u: VkLayerDeviceCreateInfoUField,
}

pub type VkNegotiateLayerStructType = std::os::raw::c_uint;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct VkNegotiateLayerInterface {
    pub s_type: VkNegotiateLayerStructType,
    pub p_next: *mut std::os::raw::c_void,
    pub loader_layer_interface_version: u32,
    pub pfn_get_instance_proc_addr: vk::FnGetInstanceProcAddr,
    pub pfn_get_device_proc_addr: vk::FnGetDeviceProcAddr,
    pub pfn_get_physical_device_proc_addr: FnGetPhysicalDeviceProcAddr,
}
