//! PHP-language name knowledge: the built-in function table and php.net doc URLs.

/// Sorted list of known PHP built-in functions. Must remain sorted for binary_search.
const BUILTINS: &[&str] = &[
    "abs",
    "acos",
    "addslashes",
    "array_chunk",
    "array_combine",
    "array_diff",
    "array_fill",
    "array_fill_keys",
    "array_filter",
    "array_flip",
    "array_intersect",
    "array_key_exists",
    "array_keys",
    "array_map",
    "array_merge",
    "array_pad",
    "array_pop",
    "array_push",
    "array_reduce",
    "array_replace",
    "array_reverse",
    "array_search",
    "array_shift",
    "array_slice",
    "array_splice",
    "array_unique",
    "array_unshift",
    "array_values",
    "array_walk",
    "array_walk_recursive",
    "arsort",
    "asin",
    "asort",
    "atan",
    "atan2",
    "base64_decode",
    "base64_encode",
    "basename",
    "boolval",
    "call_user_func",
    "call_user_func_array",
    "ceil",
    "checkdate",
    "class_exists",
    "closedir",
    "compact",
    "constant",
    "copy",
    "cos",
    "date",
    "date_add",
    "date_create",
    "date_diff",
    "date_format",
    "date_sub",
    "define",
    "defined",
    "die",
    "dirname",
    "empty",
    "exit",
    "exp",
    "explode",
    "extract",
    "fclose",
    "feof",
    "fgets",
    "file_exists",
    "file_get_contents",
    "file_put_contents",
    "floatval",
    "floor",
    "fmod",
    "fopen",
    "fputs",
    "fread",
    "fseek",
    "ftell",
    "function_exists",
    "get_class",
    "get_parent_class",
    "gettype",
    "glob",
    "hash",
    "header",
    "headers_sent",
    "htmlentities",
    "htmlspecialchars",
    "http_build_query",
    "implode",
    "in_array",
    "intdiv",
    "interface_exists",
    "intval",
    "is_a",
    "is_array",
    "is_bool",
    "is_callable",
    "is_dir",
    "is_double",
    "is_file",
    "is_finite",
    "is_float",
    "is_infinite",
    "is_int",
    "is_integer",
    "is_long",
    "is_nan",
    "is_null",
    "is_numeric",
    "is_object",
    "is_readable",
    "is_string",
    "is_subclass_of",
    "is_writable",
    "isset",
    "join",
    "json_decode",
    "json_encode",
    "krsort",
    "ksort",
    "lcfirst",
    "list",
    "log",
    "ltrim",
    "max",
    "md5",
    "method_exists",
    "microtime",
    "min",
    "mkdir",
    "mktime",
    "mt_rand",
    "nl2br",
    "number_format",
    "ob_end_clean",
    "ob_get_clean",
    "ob_start",
    "opendir",
    "parse_str",
    "parse_url",
    "pathinfo",
    "pi",
    "pow",
    "preg_match",
    "preg_match_all",
    "preg_quote",
    "preg_replace",
    "preg_split",
    "print_r",
    "printf",
    "property_exists",
    "rand",
    "random_int",
    "rawurldecode",
    "rawurlencode",
    "readdir",
    "realpath",
    "rename",
    "rewind",
    "rmdir",
    "round",
    "rsort",
    "rtrim",
    "scandir",
    "serialize",
    "session_destroy",
    "session_start",
    "setcookie",
    "settype",
    "sha1",
    "sin",
    "sleep",
    "sort",
    "sprintf",
    "sqrt",
    "str_contains",
    "str_ends_with",
    "str_pad",
    "str_repeat",
    "str_replace",
    "str_split",
    "str_starts_with",
    "str_word_count",
    "strcasecmp",
    "strcmp",
    "strip_tags",
    "stripslashes",
    "stristr",
    "strlen",
    "strncasecmp",
    "strncmp",
    "strpos",
    "strrpos",
    "strstr",
    "strtolower",
    "strtotime",
    "strtoupper",
    "strval",
    "substr",
    "substr_count",
    "substr_replace",
    "tan",
    "time",
    "trim",
    "uasort",
    "ucfirst",
    "ucwords",
    "uksort",
    "unlink",
    "unserialize",
    "unset",
    "urldecode",
    "urlencode",
    "usleep",
    "usort",
    "var_dump",
    "var_export",
    "vsprintf",
];

/// Return `true` if `name` is a known PHP built-in function.
/// Used by hover to generate php.net links.
pub(crate) fn is_php_builtin(name: &str) -> bool {
    BUILTINS.binary_search(&name).is_ok()
}

/// Build the php.net documentation URL for a built-in function name.
pub(crate) fn php_doc_url(name: &str) -> String {
    // php.net uses underscores replaced with dashes in the URL path.
    let slug = name.replace('_', "-");
    format!("https://www.php.net/function.{}", slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The BUILTINS list must be sorted for binary_search to work correctly.
    /// This catches the class of bug where a new builtin is appended out of order.
    #[test]
    fn builtins_are_sorted() {
        for w in BUILTINS.windows(2) {
            assert!(
                w[0] <= w[1],
                "BUILTINS out of order: {:?} >= {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn is_php_builtin_asin_recognized() {
        // asin was out of order in BUILTINS, causing binary_search to miss it.
        assert!(
            is_php_builtin("asin"),
            "asin must be recognised as a PHP builtin"
        );
        assert!(
            is_php_builtin("atan"),
            "atan must be recognised as a PHP builtin"
        );
        assert!(
            is_php_builtin("krsort"),
            "krsort must be recognised as a PHP builtin"
        );
        assert!(
            is_php_builtin("strcasecmp"),
            "strcasecmp must be recognised as a PHP builtin"
        );
        assert!(
            is_php_builtin("strncasecmp"),
            "strncasecmp must be recognised as a PHP builtin"
        );
        assert!(
            is_php_builtin("strip_tags"),
            "strip_tags must be recognised as a PHP builtin"
        );
    }
}
