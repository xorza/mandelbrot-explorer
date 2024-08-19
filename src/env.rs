pub fn is_test_build() -> bool {
    cfg!(test)
}

pub fn is_debug_build() -> bool {
    cfg!(debug_assertions)
}
