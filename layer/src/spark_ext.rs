use spark::vk;
use std::mem;

type VkResult<T> = Result<T, vk::Result>;

pub trait SparkResultExt {
    fn result(self) -> VkResult<()>;
    fn result_with_success<T>(self, v: T) -> VkResult<T>;
    unsafe fn assume_init_on_success<T>(self, v: mem::MaybeUninit<T>) -> VkResult<T>;
}

impl SparkResultExt for vk::Result {
    #[inline]
    fn result(self) -> VkResult<()> {
        self.result_with_success(())
    }

    #[inline]
    fn result_with_success<T>(self, v: T) -> VkResult<T> {
        match self {
            Self::SUCCESS => Ok(v),
            _ => Err(self),
        }
    }

    #[inline]
    unsafe fn assume_init_on_success<T>(self, v: mem::MaybeUninit<T>) -> VkResult<T> {
        self.result().map(move |()| v.assume_init())
    }
}
