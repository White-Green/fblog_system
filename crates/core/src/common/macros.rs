#[macro_export]
macro_rules! json_format {
    (@internal: $($key:literal : $value:tt),*$(,)?) => {
        ::std::format_args!("{{{}}}", json_format!(@internal_kv_list: $($key : $value),*))
    };
    (@internal: $($value:tt),*$(,)?) => {
        ::std::format_args!("[{}]", json_format!(@internal_value_list: $($value),*))
    };
    (@internal_kv_list: $key:literal : $value:tt, $($tail:tt)+) => {
        ::std::format_args!(json_format!(@kv_formatstring_rest: $key : $value), $key, json_format!(@internal_value: $value), json_format!(@internal_kv_list: $($tail)+))
    };
    (@internal_kv_list: $key:literal : $value:tt ,) => {
        ::std::format_args!(json_format!(@kv_formatstring: $key : $value), $key, json_format!(@internal_value: $value))
    };
    (@internal_kv_list: $key:literal : $value:tt) => {
        ::std::format_args!(json_format!(@kv_formatstring: $key : $value), $key, json_format!(@internal_value: $value))
    };
    (@internal_kv_list:) => {
        ""
    };
    (@kv_formatstring_rest: $key:literal : $value:literal) => {
        "{:?}:{:?},{}"
    };
    (@kv_formatstring_rest: $key:literal : $value:tt) => {
        "{:?}:{},{}"
    };
    (@kv_formatstring: $key:literal : $value:literal) => {
        "{:?}:{:?}"
    };
    (@kv_formatstring: $key:literal : $value:tt) => {
        "{:?}:{}"
    };
    (@internal_value_list: $value:tt, $($tail:tt)+) => {
        ::std::format_args!(json_format!(@list_formatstring_rest: $value), json_format!(@internal_value: $value), json_format!(@internal_value_list: $($tail)+))
    };
    (@internal_value_list: $value:tt ,) => {
        ::std::format_args!(json_format!(@list_formatstring: $value), json_format!(@internal_value: $value))
    };
    (@internal_value_list: $value:tt) => {
        ::std::format_args!(json_format!(@list_formatstring: $value), json_format!(@internal_value: $value))
    };
    (@internal_value_list: ) => {
        ""
    };
    (@list_formatstring_rest: $value:literal) => {
        "{:?},{}"
    };
    (@list_formatstring_rest: $value:tt) => {
        "{},{}"
    };
    (@list_formatstring: $value:literal) => {
        "{:?}"
    };
    (@list_formatstring: $value:tt) => {
        "{}"
    };
    (@internal_value: { $($t:tt)* }) => {
        ::std::format_args!("{{{}}}", json_format!(@internal_kv_list: $($t)*))
    };
    (@internal_value: [ $($t:tt)* ]) => {
        ::std::format_args!("[{}]", json_format!(@internal_value_list: $($t)*))
    };
    (@internal_value: $expr:expr) => {
        $expr
    };
    ($($t:tt)*) => {
        ::std::fmt::format(json_format!(@internal: $($t)*))
    };
}

pub use json_format;
