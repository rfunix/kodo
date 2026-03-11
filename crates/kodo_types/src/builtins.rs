//! Builtin function and method registration for the Kōdo type checker.
//!
//! Contains `register_builtins` and all helper methods for registering
//! builtin types, methods, and functions in the type environment.

use crate::checker::TypeChecker;
use crate::Type;

impl TypeChecker {
    /// Registers builtin functions in the type environment.
    ///
    /// These are functions provided by the runtime that do not need to be
    /// declared in user code. Currently registers:
    /// - `println(String) -> ()`
    /// - `print(String) -> ()`
    /// - `print_int(Int) -> ()`
    /// - String methods: `length`, `contains`, `starts_with`, `ends_with`,
    ///   `trim`, `to_upper`, `to_lower`, `substring`
    /// - Int methods: `to_string`, `to_float64`
    /// - Float64 methods: `to_string`, `to_int`
    pub(crate) fn register_builtins(&mut self) {
        self.env.insert(
            "println".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Unit)),
        );
        self.env.insert(
            "print".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Unit)),
        );
        self.env.insert(
            "print_int".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
        self.env.insert(
            "print_float".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Unit)),
        );
        self.env.insert(
            "println_float".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Unit)),
        );
        // Math builtins
        self.env.insert(
            "abs".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "min".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "max".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "clamp".to_string(),
            Type::Function(vec![Type::Int, Type::Int, Type::Int], Box::new(Type::Int)),
        );

        // File I/O builtins
        self.env.insert(
            "file_exists".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Bool)),
        );
        self.env.insert(
            "file_read".to_string(),
            Type::Function(
                vec![Type::String],
                Box::new(Type::Enum("Result__String_String".to_string())),
            ),
        );
        self.env.insert(
            "file_write".to_string(),
            Type::Function(
                vec![Type::String, Type::String],
                Box::new(Type::Enum("Result__Unit_String".to_string())),
            ),
        );

        self.register_string_methods();
        self.register_int_methods();
        self.register_float_methods();
        self.register_list_functions();
        self.register_map_functions();
        self.register_http_functions();
        self.register_json_functions();
        self.register_time_functions();
        self.register_env_functions();
        self.register_channel_functions();
    }

    /// Registers builtin methods for the `String` type.
    ///
    /// These methods are available on all String values and are implemented
    /// in the runtime as `kodo_string_*` functions.
    fn register_string_methods(&mut self) {
        self.register_string_query_methods();
        self.register_string_transform_methods();
    }

    /// Registers String query methods: `length`, `contains`, `starts_with`, `ends_with`.
    fn register_string_query_methods(&mut self) {
        // String.length() -> Int
        self.method_lookup.insert(
            ("String".to_string(), "length".to_string()),
            ("String_length".to_string(), vec![Type::String], Type::Int),
        );
        self.env.insert(
            "String_length".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Int)),
        );

        // String.contains(s: String) -> Bool
        self.method_lookup.insert(
            ("String".to_string(), "contains".to_string()),
            (
                "String_contains".to_string(),
                vec![Type::String, Type::String],
                Type::Bool,
            ),
        );
        self.env.insert(
            "String_contains".to_string(),
            Type::Function(vec![Type::String, Type::String], Box::new(Type::Bool)),
        );

        // String.starts_with(s: String) -> Bool
        self.method_lookup.insert(
            ("String".to_string(), "starts_with".to_string()),
            (
                "String_starts_with".to_string(),
                vec![Type::String, Type::String],
                Type::Bool,
            ),
        );
        self.env.insert(
            "String_starts_with".to_string(),
            Type::Function(vec![Type::String, Type::String], Box::new(Type::Bool)),
        );

        // String.ends_with(s: String) -> Bool
        self.method_lookup.insert(
            ("String".to_string(), "ends_with".to_string()),
            (
                "String_ends_with".to_string(),
                vec![Type::String, Type::String],
                Type::Bool,
            ),
        );
        self.env.insert(
            "String_ends_with".to_string(),
            Type::Function(vec![Type::String, Type::String], Box::new(Type::Bool)),
        );
    }

    /// Registers String transform methods: `trim`, `to_upper`, `to_lower`, `substring`, `split`.
    fn register_string_transform_methods(&mut self) {
        // String.trim() -> String
        self.method_lookup.insert(
            ("String".to_string(), "trim".to_string()),
            ("String_trim".to_string(), vec![Type::String], Type::String),
        );
        self.env.insert(
            "String_trim".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::String)),
        );

        // String.to_upper() -> String
        self.method_lookup.insert(
            ("String".to_string(), "to_upper".to_string()),
            (
                "String_to_upper".to_string(),
                vec![Type::String],
                Type::String,
            ),
        );
        self.env.insert(
            "String_to_upper".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::String)),
        );

        // String.to_lower() -> String
        self.method_lookup.insert(
            ("String".to_string(), "to_lower".to_string()),
            (
                "String_to_lower".to_string(),
                vec![Type::String],
                Type::String,
            ),
        );
        self.env.insert(
            "String_to_lower".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::String)),
        );

        // String.substring(start: Int, end: Int) -> String
        self.method_lookup.insert(
            ("String".to_string(), "substring".to_string()),
            (
                "String_substring".to_string(),
                vec![Type::String, Type::Int, Type::Int],
                Type::String,
            ),
        );
        self.env.insert(
            "String_substring".to_string(),
            Type::Function(
                vec![Type::String, Type::Int, Type::Int],
                Box::new(Type::String),
            ),
        );

        // String.split(sep: String) -> List<String>
        self.method_lookup.insert(
            ("String".to_string(), "split".to_string()),
            (
                "String_split".to_string(),
                vec![Type::String, Type::String],
                Type::Generic("List".to_string(), vec![Type::String]),
            ),
        );
        self.env.insert(
            "String_split".to_string(),
            Type::Function(
                vec![Type::String, Type::String],
                Box::new(Type::Generic("List".to_string(), vec![Type::String])),
            ),
        );
    }

    /// Registers builtin methods for the `Int` type.
    fn register_int_methods(&mut self) {
        // Int.to_string() -> String
        self.method_lookup.insert(
            ("Int".to_string(), "to_string".to_string()),
            ("Int_to_string".to_string(), vec![Type::Int], Type::String),
        );
        self.env.insert(
            "Int_to_string".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::String)),
        );

        // Int.to_float64() -> Float64
        self.method_lookup.insert(
            ("Int".to_string(), "to_float64".to_string()),
            ("Int_to_float64".to_string(), vec![Type::Int], Type::Float64),
        );
        self.env.insert(
            "Int_to_float64".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Float64)),
        );
    }

    /// Registers builtin methods for the `Float64` type.
    fn register_float_methods(&mut self) {
        // Float64.to_string() -> String
        self.method_lookup.insert(
            ("Float64".to_string(), "to_string".to_string()),
            (
                "Float64_to_string".to_string(),
                vec![Type::Float64],
                Type::String,
            ),
        );
        self.env.insert(
            "Float64_to_string".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::String)),
        );

        // Float64.to_int() -> Int
        self.method_lookup.insert(
            ("Float64".to_string(), "to_int".to_string()),
            ("Float64_to_int".to_string(), vec![Type::Float64], Type::Int),
        );
        self.env.insert(
            "Float64_to_int".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Int)),
        );
    }

    /// Registers builtin functions for `List<T>` operations.
    ///
    /// These are free functions (not methods) available to all Kōdo programs.
    /// At runtime, lists are opaque heap pointers managed by the runtime.
    fn register_list_functions(&mut self) {
        self.env.insert(
            "list_new".to_string(),
            Type::Function(
                vec![],
                Box::new(Type::Generic("List".to_string(), vec![Type::Int])),
            ),
        );
        self.env.insert(
            "list_push".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Unit),
            ),
        );
        self.env.insert(
            "list_get".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Int),
            ),
        );
        self.env.insert(
            "list_length".to_string(),
            Type::Function(
                vec![Type::Generic("List".to_string(), vec![Type::Int])],
                Box::new(Type::Int),
            ),
        );
        self.env.insert(
            "list_contains".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Bool),
            ),
        );
    }

    /// Registers builtin functions for `Map<K, V>` operations.
    ///
    /// Maps use integer keys and values at the runtime level. All values
    /// are represented as i64 (pointers or values).
    fn register_map_functions(&mut self) {
        self.env.insert(
            "map_new".to_string(),
            Type::Function(
                vec![],
                Box::new(Type::Generic("Map".to_string(), vec![Type::Int, Type::Int])),
            ),
        );
        self.env.insert(
            "map_insert".to_string(),
            Type::Function(
                vec![
                    Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                    Type::Int,
                    Type::Int,
                ],
                Box::new(Type::Unit),
            ),
        );
        self.env.insert(
            "map_get".to_string(),
            Type::Function(
                vec![
                    Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Int),
            ),
        );
        self.env.insert(
            "map_contains_key".to_string(),
            Type::Function(
                vec![
                    Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Bool),
            ),
        );
        self.env.insert(
            "map_length".to_string(),
            Type::Function(
                vec![Type::Generic("Map".to_string(), vec![Type::Int, Type::Int])],
                Box::new(Type::Int),
            ),
        );
    }

    /// Registers builtin functions for HTTP client operations.
    ///
    /// These functions are implemented in the runtime as `kodo_http_*` functions.
    fn register_http_functions(&mut self) {
        self.env.insert(
            "http_get".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Int)),
        );
        self.env.insert(
            "http_post".to_string(),
            Type::Function(vec![Type::String, Type::String], Box::new(Type::Int)),
        );
    }

    /// Registers builtin functions for JSON parsing operations.
    ///
    /// These functions are implemented in the runtime as `kodo_json_*` functions.
    /// JSON values are represented as opaque `Int` handles.
    fn register_json_functions(&mut self) {
        self.env.insert(
            "json_parse".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Int)),
        );
        self.env.insert(
            "json_get_string".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::Int)),
        );
        self.env.insert(
            "json_get_int".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::Int)),
        );
        self.env.insert(
            "json_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
    }

    /// Registers builtin functions for time operations.
    fn register_time_functions(&mut self) {
        self.env.insert(
            "time_now".to_string(),
            Type::Function(vec![], Box::new(Type::Int)),
        );
        self.env.insert(
            "time_now_ms".to_string(),
            Type::Function(vec![], Box::new(Type::Int)),
        );
        self.env.insert(
            "time_format".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::String)),
        );
        self.env.insert(
            "time_elapsed_ms".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
    }

    /// Registers builtin functions for environment variable access.
    fn register_env_functions(&mut self) {
        self.env.insert(
            "env_get".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::String)),
        );
        self.env.insert(
            "env_set".to_string(),
            Type::Function(vec![Type::String, Type::String], Box::new(Type::Unit)),
        );
    }

    /// Registers builtin functions for inter-thread channel communication.
    fn register_channel_functions(&mut self) {
        self.env.insert(
            "channel_new".to_string(),
            Type::Function(vec![], Box::new(Type::Int)),
        );
        self.env.insert(
            "channel_send".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Unit)),
        );
        self.env.insert(
            "channel_recv".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "channel_send_bool".to_string(),
            Type::Function(vec![Type::Int, Type::Bool], Box::new(Type::Unit)),
        );
        self.env.insert(
            "channel_recv_bool".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Bool)),
        );
        self.env.insert(
            "channel_send_string".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::Unit)),
        );
        self.env.insert(
            "channel_recv_string".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::String)),
        );
        self.env.insert(
            "channel_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
    }
}
