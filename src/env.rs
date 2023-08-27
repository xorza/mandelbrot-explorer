pub fn is_test_build() -> bool {
    if cfg!(test) {
        true
    } else {
        false
    }
}


pub fn is_debug_build() -> bool {
    if cfg!(debug_assertions) {
        true
    } else {
        false
    }
}