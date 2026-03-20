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
    /// - Test assertion builtins: `assert`, `assert_true`, `assert_false`
    ///   (`assert_eq` and `assert_ne` are handled specially in `check_call`)
    /// - Property testing builtins: `kodo_prop_start`, `kodo_prop_gen_int`, etc.
    /// - Timeout builtins: `kodo_test_set_timeout`, `kodo_test_clear_timeout`
    /// - Isolation builtins: `kodo_test_isolate_start`, `kodo_test_isolate_end`
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
        self.register_bool_methods();
        self.register_option_methods();
        self.register_result_methods();
        self.register_list_functions();
        self.register_iterator_functions();
        self.register_combinator_methods();
        self.register_map_functions();
        self.register_http_functions();
        self.register_json_functions();
        self.register_time_functions();
        self.register_env_functions();
        self.register_channel_functions();
        self.register_cli_functions();
        self.register_file_extended_functions();
        self.register_json_builder_functions();
        self.register_math_extended_functions();
        self.register_http_server_functions();
        self.register_db_functions();

        self.register_future_builtins();
        self.register_test_builtins();
    }

    /// Registers builtin functions for Future/async operations.
    ///
    /// These are low-level runtime functions used by the codegen to implement
    /// `async fn` and `await` expressions. User code interacts with them
    /// indirectly through the `async`/`await` syntax.
    fn register_future_builtins(&mut self) {
        // kodo_future_new() -> Int (opaque future handle)
        self.env.insert(
            "kodo_future_new".to_string(),
            Type::Function(vec![], Box::new(Type::Int)),
        );
        // kodo_future_complete(handle: Int, result: Int) -> ()
        self.env.insert(
            "kodo_future_complete".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Unit)),
        );
        // kodo_future_await(handle: Int) -> Int
        self.env.insert(
            "kodo_future_await".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        // kodo_future_complete_bytes(handle: Int, data_ptr: Int, data_size: Int) -> ()
        self.env.insert(
            "kodo_future_complete_bytes".to_string(),
            Type::Function(vec![Type::Int, Type::Int, Type::Int], Box::new(Type::Unit)),
        );
        // kodo_future_await_bytes(handle: Int, out_ptr: Int, data_size: Int) -> ()
        self.env.insert(
            "kodo_future_await_bytes".to_string(),
            Type::Function(vec![Type::Int, Type::Int, Type::Int], Box::new(Type::Unit)),
        );
    }

    /// Registers test assertion, harness, property, timeout, and isolation builtins.
    ///
    /// - `assert`, `assert_true`, `assert_false` — standard assertion builtins
    ///   (`assert_eq`/`assert_ne` are polymorphic and handled in `check_call`)
    /// - `kodo_test_start`, `kodo_test_end`, `kodo_test_summary` — test harness runtime
    /// - `kodo_prop_start`, `kodo_prop_gen_*` — property testing generators
    /// - `kodo_test_set_timeout`, `kodo_test_clear_timeout` — timeout support
    /// - `kodo_test_isolate_start`, `kodo_test_isolate_end` — test isolation support
    fn register_test_builtins(&mut self) {
        // Test assertion builtins — assert_eq/assert_ne are polymorphic and
        // handled as special cases in `check_call`.
        self.env.insert(
            "assert".to_string(),
            Type::Function(vec![Type::Bool], Box::new(Type::Unit)),
        );
        self.env.insert(
            "assert_true".to_string(),
            Type::Function(vec![Type::Bool], Box::new(Type::Unit)),
        );
        self.env.insert(
            "assert_false".to_string(),
            Type::Function(vec![Type::Bool], Box::new(Type::Unit)),
        );

        // Test harness runtime builtins — used by the synthetic `main` in test mode.
        self.env.insert(
            "kodo_test_start".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Unit)),
        );
        self.env.insert(
            "kodo_test_end".to_string(),
            Type::Function(vec![], Box::new(Type::Int)),
        );
        self.env.insert(
            "kodo_test_skip".to_string(),
            Type::Function(vec![], Box::new(Type::Unit)),
        );
        self.env.insert(
            "kodo_test_summary".to_string(),
            Type::Function(
                vec![Type::Int, Type::Int, Type::Int, Type::Int, Type::Int],
                Box::new(Type::Unit),
            ),
        );

        self.register_property_testing_builtins();
    }

    /// Registers property testing, timeout, and isolation builtins.
    ///
    /// These builtins are emitted by the `forall` desugaring pass and by
    /// `@timeout`/`@isolate` annotation desugaring. They must be resolvable
    /// in the type environment so that generated code type-checks.
    fn register_property_testing_builtins(&mut self) {
        // kodo_prop_start(seed: Int, iterations: Int) -> ()
        self.env.insert(
            "kodo_prop_start".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Unit)),
        );
        // kodo_prop_gen_int(min: Int, max: Int) -> Int
        self.env.insert(
            "kodo_prop_gen_int".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int)),
        );
        // kodo_prop_gen_bool() -> Bool
        self.env.insert(
            "kodo_prop_gen_bool".to_string(),
            Type::Function(vec![], Box::new(Type::Bool)),
        );
        // kodo_prop_gen_float(min: Float64, max: Float64) -> Float64
        self.env.insert(
            "kodo_prop_gen_float".to_string(),
            Type::Function(vec![Type::Float64, Type::Float64], Box::new(Type::Float64)),
        );
        // kodo_prop_gen_string(max_len: Int) -> String
        self.env.insert(
            "kodo_prop_gen_string".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::String)),
        );
        // kodo_test_set_timeout(ms: Int) -> ()
        self.env.insert(
            "kodo_test_set_timeout".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
        // kodo_test_clear_timeout() -> ()
        self.env.insert(
            "kodo_test_clear_timeout".to_string(),
            Type::Function(vec![], Box::new(Type::Unit)),
        );
        // kodo_test_isolate_start() -> ()
        self.env.insert(
            "kodo_test_isolate_start".to_string(),
            Type::Function(vec![], Box::new(Type::Unit)),
        );
        // kodo_test_isolate_end() -> ()
        self.env.insert(
            "kodo_test_isolate_end".to_string(),
            Type::Function(vec![], Box::new(Type::Unit)),
        );
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
        // String.length() -> Int (returns Unicode code point count)
        self.method_lookup.insert(
            ("String".to_string(), "length".to_string()),
            ("String_length".to_string(), vec![Type::String], Type::Int),
        );
        self.env.insert(
            "String_length".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Int)),
        );

        // String.byte_length() -> Int (returns byte count)
        self.method_lookup.insert(
            ("String".to_string(), "byte_length".to_string()),
            (
                "String_byte_length".to_string(),
                vec![Type::String],
                Type::Int,
            ),
        );
        self.env.insert(
            "String_byte_length".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Int)),
        );

        // String.char_count() -> Int (alias for length — Unicode code point count)
        self.method_lookup.insert(
            ("String".to_string(), "char_count".to_string()),
            (
                "String_char_count".to_string(),
                vec![Type::String],
                Type::Int,
            ),
        );
        self.env.insert(
            "String_char_count".to_string(),
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

    /// Registers String transform methods: `trim`, `to_upper`, `to_lower`, `substring`, `split`,
    /// `char_at`, `repeat`.
    #[allow(clippy::too_many_lines)]
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

        // String.lines() -> List<String>
        self.method_lookup.insert(
            ("String".to_string(), "lines".to_string()),
            (
                "String_lines".to_string(),
                vec![Type::String],
                Type::Generic("List".to_string(), vec![Type::String]),
            ),
        );
        self.env.insert(
            "String_lines".to_string(),
            Type::Function(
                vec![Type::String],
                Box::new(Type::Generic("List".to_string(), vec![Type::String])),
            ),
        );

        // String.parse_int() -> Int
        self.method_lookup.insert(
            ("String".to_string(), "parse_int".to_string()),
            (
                "String_parse_int".to_string(),
                vec![Type::String],
                Type::Int,
            ),
        );
        self.env.insert(
            "String_parse_int".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Int)),
        );

        // String.char_at(index: Int) -> Int
        self.method_lookup.insert(
            ("String".to_string(), "char_at".to_string()),
            (
                "String_char_at".to_string(),
                vec![Type::String, Type::Int],
                Type::Int,
            ),
        );
        self.env.insert(
            "String_char_at".to_string(),
            Type::Function(vec![Type::String, Type::Int], Box::new(Type::Int)),
        );

        // String.repeat(count: Int) -> String
        self.method_lookup.insert(
            ("String".to_string(), "repeat".to_string()),
            (
                "String_repeat".to_string(),
                vec![Type::String, Type::Int],
                Type::String,
            ),
        );
        self.env.insert(
            "String_repeat".to_string(),
            Type::Function(vec![Type::String, Type::Int], Box::new(Type::String)),
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

    /// Registers builtin methods for the `Bool` type.
    fn register_bool_methods(&mut self) {
        // Bool.to_string() -> String
        self.method_lookup.insert(
            ("Bool".to_string(), "to_string".to_string()),
            ("Bool_to_string".to_string(), vec![Type::Bool], Type::String),
        );
        self.env.insert(
            "Bool_to_string".to_string(),
            Type::Function(vec![Type::Bool], Box::new(Type::String)),
        );
    }

    /// Registers builtin methods for `Option<T>`.
    ///
    /// Methods: `is_some`, `is_none`, `unwrap`, `unwrap_or`.
    /// These are implemented in the runtime and work on the enum tag.
    fn register_option_methods(&mut self) {
        let option_ty = Type::Enum("Option".to_string());

        // Option.is_some() -> Bool
        self.method_lookup.insert(
            ("Option".to_string(), "is_some".to_string()),
            (
                "Option_is_some".to_string(),
                vec![option_ty.clone()],
                Type::Bool,
            ),
        );
        self.env.insert(
            "Option_is_some".to_string(),
            Type::Function(vec![option_ty.clone()], Box::new(Type::Bool)),
        );

        // Option.is_none() -> Bool
        self.method_lookup.insert(
            ("Option".to_string(), "is_none".to_string()),
            (
                "Option_is_none".to_string(),
                vec![option_ty.clone()],
                Type::Bool,
            ),
        );
        self.env.insert(
            "Option_is_none".to_string(),
            Type::Function(vec![option_ty.clone()], Box::new(Type::Bool)),
        );

        // Option.unwrap() -> T (resolved polymorphically in try_check_method_call)
        // Registered with Int as placeholder; actual return type comes from Generic params.
        self.method_lookup.insert(
            ("Option".to_string(), "unwrap".to_string()),
            (
                "Option_unwrap".to_string(),
                vec![option_ty.clone()],
                Type::Int,
            ),
        );
        self.env.insert(
            "Option_unwrap".to_string(),
            Type::Function(vec![option_ty.clone()], Box::new(Type::Int)),
        );

        // Option.unwrap_or(default: Int) -> Int
        self.method_lookup.insert(
            ("Option".to_string(), "unwrap_or".to_string()),
            (
                "Option_unwrap_or".to_string(),
                vec![option_ty.clone(), Type::Int],
                Type::Int,
            ),
        );
        self.env.insert(
            "Option_unwrap_or".to_string(),
            Type::Function(vec![option_ty, Type::Int], Box::new(Type::Int)),
        );
    }

    /// Registers builtin methods for `Result<T, E>`.
    ///
    /// Methods: `is_ok`, `is_err`, `unwrap`, `unwrap_err`, `unwrap_or`.
    /// These are implemented in the runtime and work on the enum tag.
    fn register_result_methods(&mut self) {
        let result_ty = Type::Enum("Result".to_string());

        // Result.is_ok() -> Bool
        self.method_lookup.insert(
            ("Result".to_string(), "is_ok".to_string()),
            (
                "Result_is_ok".to_string(),
                vec![result_ty.clone()],
                Type::Bool,
            ),
        );
        self.env.insert(
            "Result_is_ok".to_string(),
            Type::Function(vec![result_ty.clone()], Box::new(Type::Bool)),
        );

        // Result.is_err() -> Bool
        self.method_lookup.insert(
            ("Result".to_string(), "is_err".to_string()),
            (
                "Result_is_err".to_string(),
                vec![result_ty.clone()],
                Type::Bool,
            ),
        );
        self.env.insert(
            "Result_is_err".to_string(),
            Type::Function(vec![result_ty.clone()], Box::new(Type::Bool)),
        );

        // Result.unwrap() -> T (resolved polymorphically in try_check_method_call)
        // Registered with Int as placeholder; actual return type comes from Generic params.
        self.method_lookup.insert(
            ("Result".to_string(), "unwrap".to_string()),
            (
                "Result_unwrap".to_string(),
                vec![result_ty.clone()],
                Type::Int,
            ),
        );
        self.env.insert(
            "Result_unwrap".to_string(),
            Type::Function(vec![result_ty.clone()], Box::new(Type::Int)),
        );

        // Result.unwrap_err() -> E (resolved polymorphically in try_check_method_call)
        // Registered with String as placeholder; actual return type comes from Generic params.
        self.method_lookup.insert(
            ("Result".to_string(), "unwrap_err".to_string()),
            (
                "Result_unwrap_err".to_string(),
                vec![result_ty.clone()],
                Type::String,
            ),
        );
        self.env.insert(
            "Result_unwrap_err".to_string(),
            Type::Function(vec![result_ty.clone()], Box::new(Type::String)),
        );

        // Result.unwrap_or(default: Int) -> Int
        self.method_lookup.insert(
            ("Result".to_string(), "unwrap_or".to_string()),
            (
                "Result_unwrap_or".to_string(),
                vec![result_ty.clone(), Type::Int],
                Type::Int,
            ),
        );
        self.env.insert(
            "Result_unwrap_or".to_string(),
            Type::Function(vec![result_ty, Type::Int], Box::new(Type::Int)),
        );
    }

    /// Registers builtin functions for `List<T>` operations.
    ///
    /// These are free functions (not methods) available to all Kōdo programs.
    /// At runtime, lists are opaque heap pointers managed by the runtime.
    fn register_list_functions(&mut self) {
        self.register_list_core_functions();
        self.register_list_methods();
    }

    /// Registers core list functions (new, push, get, length, etc.).
    fn register_list_core_functions(&mut self) {
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
        // list_pop(List<Int>) -> Int (uses out-params at runtime, returns last element or 0)
        self.env.insert(
            "list_pop".to_string(),
            Type::Function(
                vec![Type::Generic("List".to_string(), vec![Type::Int])],
                Box::new(Type::Int),
            ),
        );
        // list_remove(List<Int>, Int) -> Bool (returns true if index was valid)
        self.env.insert(
            "list_remove".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Bool),
            ),
        );
        // list_set(List<Int>, Int, Int) -> Bool (returns true if index was valid)
        self.env.insert(
            "list_set".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                    Type::Int,
                ],
                Box::new(Type::Bool),
            ),
        );
        // list_is_empty(List<Int>) -> Bool
        self.env.insert(
            "list_is_empty".to_string(),
            Type::Function(
                vec![Type::Generic("List".to_string(), vec![Type::Int])],
                Box::new(Type::Bool),
            ),
        );
        // list_reverse(List<Int>) -> ()  (reverses in place)
        self.env.insert(
            "list_reverse".to_string(),
            Type::Function(
                vec![Type::Generic("List".to_string(), vec![Type::Int])],
                Box::new(Type::Unit),
            ),
        );
    }

    /// Registers list method builtins (slice, sort, join).
    fn register_list_methods(&mut self) {
        // list_slice(List<Int>, Int, Int) -> List<Int>
        self.method_lookup.insert(
            ("List".to_string(), "slice".to_string()),
            (
                "list_slice".to_string(),
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                    Type::Int,
                ],
                Type::Generic("List".to_string(), vec![Type::Int]),
            ),
        );
        self.env.insert(
            "list_slice".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::Int]),
                    Type::Int,
                    Type::Int,
                ],
                Box::new(Type::Generic("List".to_string(), vec![Type::Int])),
            ),
        );
        // list_sort(List<Int>) -> ()  (sorts in place)
        self.method_lookup.insert(
            ("List".to_string(), "sort".to_string()),
            (
                "list_sort".to_string(),
                vec![Type::Generic("List".to_string(), vec![Type::Int])],
                Type::Unit,
            ),
        );
        self.env.insert(
            "list_sort".to_string(),
            Type::Function(
                vec![Type::Generic("List".to_string(), vec![Type::Int])],
                Box::new(Type::Unit),
            ),
        );
        // list_join(List<String>, String) -> String
        self.method_lookup.insert(
            ("List".to_string(), "join".to_string()),
            (
                "list_join".to_string(),
                vec![
                    Type::Generic("List".to_string(), vec![Type::String]),
                    Type::String,
                ],
                Type::String,
            ),
        );
        self.env.insert(
            "list_join".to_string(),
            Type::Function(
                vec![
                    Type::Generic("List".to_string(), vec![Type::String]),
                    Type::String,
                ],
                Box::new(Type::String),
            ),
        );
    }

    /// Registers builtin functions for the Iterator protocol.
    ///
    /// These are free functions used by the for-in desugaring and available
    /// for user code. They provide the Iterator protocol over Lists.
    fn register_iterator_functions(&mut self) {
        let list_ty = Type::Generic("List".to_string(), vec![Type::Int]);

        // List.iter() -> Int (returns opaque iterator handle)
        self.method_lookup.insert(
            ("List".to_string(), "iter".to_string()),
            ("list_iter".to_string(), vec![list_ty.clone()], Type::Int),
        );
        self.env.insert(
            "list_iter".to_string(),
            Type::Function(vec![list_ty], Box::new(Type::Int)),
        );

        // list_iterator_next(iter_handle: Int) -> Int (returns value, uses out-params at runtime)
        self.env.insert(
            "list_iterator_next".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );

        // list_iterator_advance(iter_handle: Int) -> Int (1 if element available, 0 if done)
        self.env.insert(
            "list_iterator_advance".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );

        // list_iterator_value(iter_handle: Int) -> Int (current element after advance)
        self.env.insert(
            "list_iterator_value".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );

        // list_iterator_free(iter_handle: Int) -> ()
        self.env.insert(
            "list_iterator_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );

        // String.chars() -> Int (opaque iterator handle)
        self.method_lookup.insert(
            ("String".to_string(), "chars".to_string()),
            ("String_chars".to_string(), vec![Type::String], Type::Int),
        );
        self.env.insert(
            "String_chars".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Int)),
        );

        // String char iterator advance/value/free
        self.env.insert(
            "string_chars_advance".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "string_chars_value".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "string_chars_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );

        let map_ty = Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]);

        // Map.keys() -> Int (opaque iterator handle)
        self.method_lookup.insert(
            ("Map".to_string(), "keys".to_string()),
            ("Map_keys".to_string(), vec![map_ty.clone()], Type::Int),
        );
        self.env.insert(
            "Map_keys".to_string(),
            Type::Function(vec![map_ty.clone()], Box::new(Type::Int)),
        );

        // Map key iterator advance/value/free
        self.env.insert(
            "map_keys_advance".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "map_keys_value".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "map_keys_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );

        // Map.values() -> Int (opaque iterator handle)
        self.method_lookup.insert(
            ("Map".to_string(), "values".to_string()),
            ("Map_values".to_string(), vec![map_ty.clone()], Type::Int),
        );
        self.env.insert(
            "Map_values".to_string(),
            Type::Function(vec![map_ty], Box::new(Type::Int)),
        );

        // Map value iterator advance/value/free
        self.env.insert(
            "map_values_advance".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "map_values_value".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        self.env.insert(
            "map_values_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
    }

    /// Registers functional combinator methods on `List<Int>`.
    ///
    /// These methods are resolved by the type checker and implemented as
    /// synthetic AST functions injected in the compiler pipeline. They use
    /// the Iterator protocol internally.
    #[allow(clippy::too_many_lines)]
    fn register_combinator_methods(&mut self) {
        let list_ty = Type::Generic("List".to_string(), vec![Type::Int]);
        let fn_int_to_int = Type::Function(vec![Type::Int], Box::new(Type::Int));
        let fn_int_to_bool = Type::Function(vec![Type::Int], Box::new(Type::Bool));
        let fn_acc_int_to_int = Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int));

        // List.map(f: (Int) -> Int) -> List<Int>
        self.method_lookup.insert(
            ("List".to_string(), "map".to_string()),
            (
                "List_map".to_string(),
                vec![list_ty.clone(), fn_int_to_int.clone()],
                list_ty.clone(),
            ),
        );
        self.env.insert(
            "List_map".to_string(),
            Type::Function(
                vec![list_ty.clone(), fn_int_to_int],
                Box::new(list_ty.clone()),
            ),
        );

        // List.filter(f: (Int) -> Bool) -> List<Int>
        self.method_lookup.insert(
            ("List".to_string(), "filter".to_string()),
            (
                "List_filter".to_string(),
                vec![list_ty.clone(), fn_int_to_bool.clone()],
                list_ty.clone(),
            ),
        );
        self.env.insert(
            "List_filter".to_string(),
            Type::Function(
                vec![list_ty.clone(), fn_int_to_bool.clone()],
                Box::new(list_ty.clone()),
            ),
        );

        // List.fold(init: Int, f: (Int, Int) -> Int) -> Int
        self.method_lookup.insert(
            ("List".to_string(), "fold".to_string()),
            (
                "List_fold".to_string(),
                vec![list_ty.clone(), Type::Int, fn_acc_int_to_int.clone()],
                Type::Int,
            ),
        );
        self.env.insert(
            "List_fold".to_string(),
            Type::Function(
                vec![list_ty.clone(), Type::Int, fn_acc_int_to_int.clone()],
                Box::new(Type::Int),
            ),
        );

        // List.reduce(init: Int, f: (Int, Int) -> Int) -> Int — alias for fold
        self.method_lookup.insert(
            ("List".to_string(), "reduce".to_string()),
            (
                "List_reduce".to_string(),
                vec![list_ty.clone(), Type::Int, fn_acc_int_to_int.clone()],
                Type::Int,
            ),
        );
        self.env.insert(
            "List_reduce".to_string(),
            Type::Function(
                vec![list_ty.clone(), Type::Int, fn_acc_int_to_int],
                Box::new(Type::Int),
            ),
        );

        // List.count(f: (Int) -> Bool) -> Int — count elements satisfying predicate
        self.method_lookup.insert(
            ("List".to_string(), "count".to_string()),
            (
                "List_count".to_string(),
                vec![list_ty.clone(), fn_int_to_bool.clone()],
                Type::Int,
            ),
        );
        self.env.insert(
            "List_count".to_string(),
            Type::Function(
                vec![list_ty.clone(), fn_int_to_bool.clone()],
                Box::new(Type::Int),
            ),
        );

        // List.any(f: (Int) -> Bool) -> Bool
        self.method_lookup.insert(
            ("List".to_string(), "any".to_string()),
            (
                "List_any".to_string(),
                vec![list_ty.clone(), fn_int_to_bool.clone()],
                Type::Bool,
            ),
        );
        self.env.insert(
            "List_any".to_string(),
            Type::Function(
                vec![list_ty.clone(), fn_int_to_bool.clone()],
                Box::new(Type::Bool),
            ),
        );

        // List.all(f: (Int) -> Bool) -> Bool
        self.method_lookup.insert(
            ("List".to_string(), "all".to_string()),
            (
                "List_all".to_string(),
                vec![list_ty.clone(), fn_int_to_bool],
                Type::Bool,
            ),
        );
        self.env.insert(
            "List_all".to_string(),
            Type::Function(vec![list_ty], Box::new(Type::Bool)),
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
        // map_remove(Map<Int, Int>, Int) -> Bool
        self.method_lookup.insert(
            ("Map".to_string(), "remove".to_string()),
            (
                "map_remove".to_string(),
                vec![
                    Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                    Type::Int,
                ],
                Type::Bool,
            ),
        );
        self.env.insert(
            "map_remove".to_string(),
            Type::Function(
                vec![
                    Type::Generic("Map".to_string(), vec![Type::Int, Type::Int]),
                    Type::Int,
                ],
                Box::new(Type::Bool),
            ),
        );
        // map_is_empty(Map<Int, Int>) -> Bool
        self.method_lookup.insert(
            ("Map".to_string(), "is_empty".to_string()),
            (
                "map_is_empty".to_string(),
                vec![Type::Generic("Map".to_string(), vec![Type::Int, Type::Int])],
                Type::Bool,
            ),
        );
        self.env.insert(
            "map_is_empty".to_string(),
            Type::Function(
                vec![Type::Generic("Map".to_string(), vec![Type::Int, Type::Int])],
                Box::new(Type::Bool),
            ),
        );
    }

    /// Registers builtin functions for HTTP client operations.
    ///
    /// These functions are implemented in the runtime as `kodo_http_*` functions.
    fn register_http_functions(&mut self) {
        self.env.insert(
            "http_get".to_string(),
            Type::Function(
                vec![Type::String],
                Box::new(Type::Enum("Result__String_String".to_string())),
            ),
        );
        self.env.insert(
            "http_post".to_string(),
            Type::Function(
                vec![Type::String, Type::String],
                Box::new(Type::Enum("Result__String_String".to_string())),
            ),
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
        // json_get_string: returns the string value from a JSON object by key.
        // The codegen handles this as a string-returning builtin (out-params).
        self.env.insert(
            "json_get_string".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::String)),
        );
        self.env.insert(
            "json_get_int".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::Int)),
        );
        self.env.insert(
            "json_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
        // json_stringify(handle: Int) -> String
        self.env.insert(
            "json_stringify".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::String)),
        );
        // json_get_bool(handle: Int, key: String) -> Int (-1 if not found)
        self.env.insert(
            "json_get_bool".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::Int)),
        );
        // json_get_float(handle: Int, key: String) -> Float64
        self.env.insert(
            "json_get_float".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::Float64)),
        );
        // json_get_array(handle: Int, key: String) -> List<Int> (list of JSON handles)
        self.env.insert(
            "json_get_array".to_string(),
            Type::Function(
                vec![Type::Int, Type::String],
                Box::new(Type::Generic("List".to_string(), vec![Type::Int])),
            ),
        );
        // json_get_object(handle: Int, key: String) -> Int (JSON handle)
        self.env.insert(
            "json_get_object".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::Int)),
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
    ///
    /// Channels support three element types: `Int`, `Bool`, and `String`.
    /// At the type level, `channel_new` returns a `Channel<Int>` by default.
    /// Typed send/recv functions accept both the opaque `Int` handle and the
    /// generic `Channel<T>` type for flexibility:
    ///
    /// - `channel_send(ch, value: Int)` / `channel_recv(ch) -> Int`
    /// - `channel_send_bool(ch, value: Bool)` / `channel_recv_bool(ch) -> Bool`
    /// - `channel_send_string(ch, value: String)` / `channel_recv_string(ch) -> String`
    fn register_channel_functions(&mut self) {
        let ch_int = Type::Generic("Channel".to_string(), vec![Type::Int]);
        let ch_bool = Type::Generic("Channel".to_string(), vec![Type::Bool]);
        let ch_string = Type::Generic("Channel".to_string(), vec![Type::String]);

        // channel_new() -> Channel<Int>  (returns generic channel type)
        self.env.insert(
            "channel_new".to_string(),
            Type::Function(vec![], Box::new(ch_int.clone())),
        );
        // channel_new_bool() -> Channel<Bool>
        self.env.insert(
            "channel_new_bool".to_string(),
            Type::Function(vec![], Box::new(ch_bool.clone())),
        );
        // channel_new_string() -> Channel<String>
        self.env.insert(
            "channel_new_string".to_string(),
            Type::Function(vec![], Box::new(ch_string.clone())),
        );

        // Int channel: channel_send(ch: Channel<Int>, value: Int) -> ()
        self.env.insert(
            "channel_send".to_string(),
            Type::Function(vec![ch_int.clone(), Type::Int], Box::new(Type::Unit)),
        );
        // channel_recv(ch: Channel<Int>) -> Int
        self.env.insert(
            "channel_recv".to_string(),
            Type::Function(vec![ch_int.clone()], Box::new(Type::Int)),
        );

        // Bool channel: channel_send_bool(ch: Channel<Bool>, value: Bool) -> ()
        self.env.insert(
            "channel_send_bool".to_string(),
            Type::Function(vec![ch_bool.clone(), Type::Bool], Box::new(Type::Unit)),
        );
        // channel_recv_bool(ch: Channel<Bool>) -> Bool
        self.env.insert(
            "channel_recv_bool".to_string(),
            Type::Function(vec![ch_bool], Box::new(Type::Bool)),
        );

        // String channel: channel_send_string(ch: Channel<String>, value: String) -> ()
        self.env.insert(
            "channel_send_string".to_string(),
            Type::Function(vec![ch_string.clone(), Type::String], Box::new(Type::Unit)),
        );
        // channel_recv_string(ch: Channel<String>) -> String
        self.env.insert(
            "channel_recv_string".to_string(),
            Type::Function(vec![ch_string], Box::new(Type::String)),
        );

        // channel_free(ch: Int) -> ()  (works on all channel types via opaque handle)
        self.env.insert(
            "channel_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );

        // channel_select_2(ch1: Channel<Int>, ch2: Channel<Int>) -> Int
        // Waits on 2 channels, returns 0 or 1 indicating which has data.
        self.env.insert(
            "channel_select_2".to_string(),
            Type::Function(vec![ch_int.clone(), ch_int.clone()], Box::new(Type::Int)),
        );
        // channel_select_3(ch1: Channel<Int>, ch2: Channel<Int>, ch3: Channel<Int>) -> Int
        // Waits on 3 channels, returns 0, 1, or 2 indicating which has data.
        self.env.insert(
            "channel_select_3".to_string(),
            Type::Function(
                vec![ch_int.clone(), ch_int.clone(), ch_int],
                Box::new(Type::Int),
            ),
        );

        // Generic channel functions — type-erased channels for any type T.
        // channel_generic_new() -> Int (opaque handle)
        self.env.insert(
            "channel_generic_new".to_string(),
            Type::Function(vec![], Box::new(Type::Int)),
        );
        // channel_generic_send(handle: Int, data_ptr: Int, data_size: Int) -> ()
        self.env.insert(
            "channel_generic_send".to_string(),
            Type::Function(vec![Type::Int, Type::Int, Type::Int], Box::new(Type::Unit)),
        );
        // channel_generic_recv(handle: Int, out_ptr: Int, data_size: Int) -> Int
        self.env.insert(
            "channel_generic_recv".to_string(),
            Type::Function(vec![Type::Int, Type::Int, Type::Int], Box::new(Type::Int)),
        );
        // channel_generic_free(handle: Int) -> ()
        self.env.insert(
            "channel_generic_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
    }

    /// Registers CLI builtins: args, readln, exit.
    fn register_cli_functions(&mut self) {
        // args() -> List<String>
        self.env.insert(
            "args".to_string(),
            Type::Function(
                vec![],
                Box::new(Type::Generic("List".to_string(), vec![Type::String])),
            ),
        );
        // readln() -> String
        self.env.insert(
            "readln".to_string(),
            Type::Function(vec![], Box::new(Type::String)),
        );
        // exit(Int) -> Unit
        self.env.insert(
            "exit".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
    }

    /// Registers extended file I/O builtins: append, delete, `dir_list`, `dir_exists`.
    fn register_file_extended_functions(&mut self) {
        // file_append(String, String) -> Result<Unit, String>
        self.env.insert(
            "file_append".to_string(),
            Type::Function(
                vec![Type::String, Type::String],
                Box::new(Type::Enum("Result__Unit_String".to_string())),
            ),
        );
        // file_delete(String) -> Bool
        self.env.insert(
            "file_delete".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Bool)),
        );
        // dir_list(String) -> List<String>
        self.env.insert(
            "dir_list".to_string(),
            Type::Function(
                vec![Type::String],
                Box::new(Type::Generic("List".to_string(), vec![Type::String])),
            ),
        );
        // dir_exists(String) -> Bool
        self.env.insert(
            "dir_exists".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Bool)),
        );
    }

    /// Registers JSON builder builtins: `new_object`, `set_string`, `set_int`, `set_bool`.
    fn register_json_builder_functions(&mut self) {
        // json_new_object() -> Int
        self.env.insert(
            "json_new_object".to_string(),
            Type::Function(vec![], Box::new(Type::Int)),
        );
        // json_set_string(Int, String, String) -> Unit
        self.env.insert(
            "json_set_string".to_string(),
            Type::Function(
                vec![Type::Int, Type::String, Type::String],
                Box::new(Type::Unit),
            ),
        );
        // json_set_int(Int, String, Int) -> Unit
        self.env.insert(
            "json_set_int".to_string(),
            Type::Function(
                vec![Type::Int, Type::String, Type::Int],
                Box::new(Type::Unit),
            ),
        );
        // json_set_bool(Int, String, Bool) -> Unit
        self.env.insert(
            "json_set_bool".to_string(),
            Type::Function(
                vec![Type::Int, Type::String, Type::Bool],
                Box::new(Type::Unit),
            ),
        );
        // json_set_float(Int, String, Float64) -> Unit
        self.env.insert(
            "json_set_float".to_string(),
            Type::Function(
                vec![Type::Int, Type::String, Type::Float64],
                Box::new(Type::Unit),
            ),
        );
    }

    /// Registers extended math builtins: `sqrt`, `pow`, trig, `floor`, `ceil`, `round`, `rand_int`.
    fn register_math_extended_functions(&mut self) {
        // sqrt(Float64) -> Float64
        self.env.insert(
            "sqrt".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Float64)),
        );
        // pow(Float64, Float64) -> Float64
        self.env.insert(
            "pow".to_string(),
            Type::Function(vec![Type::Float64, Type::Float64], Box::new(Type::Float64)),
        );
        // sin(Float64) -> Float64
        self.env.insert(
            "sin".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Float64)),
        );
        // cos(Float64) -> Float64
        self.env.insert(
            "cos".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Float64)),
        );
        // log(Float64) -> Float64
        self.env.insert(
            "log".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Float64)),
        );
        // floor(Float64) -> Float64
        self.env.insert(
            "floor".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Float64)),
        );
        // ceil(Float64) -> Float64
        self.env.insert(
            "ceil".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Float64)),
        );
        // round(Float64) -> Float64
        self.env.insert(
            "round".to_string(),
            Type::Function(vec![Type::Float64], Box::new(Type::Float64)),
        );
        // rand_int(Int, Int) -> Int
        self.env.insert(
            "rand_int".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int)),
        );
    }

    /// Registers `SQLite` database builtins.
    ///
    /// Provides functions for opening databases, executing SQL statements,
    /// querying rows, and reading column values. Handles are opaque `Int` values.
    fn register_db_functions(&mut self) {
        // db_open(path: String) -> Int (handle)
        self.env.insert(
            "db_open".to_string(),
            Type::Function(vec![Type::String], Box::new(Type::Int)),
        );
        // db_execute(db: Int, sql: String) -> Int (0=ok, 1=err)
        self.env.insert(
            "db_execute".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::Int)),
        );
        // db_query(db: Int, sql: String) -> Int (result handle)
        self.env.insert(
            "db_query".to_string(),
            Type::Function(vec![Type::Int, Type::String], Box::new(Type::Int)),
        );
        // db_row_next(result: Int) -> Int (1=has row, 0=done)
        self.env.insert(
            "db_row_next".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        // db_row_get_string(result: Int, col: Int) -> String
        self.env.insert(
            "db_row_get_string".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::String)),
        );
        // db_row_get_int(result: Int, col: Int) -> Int
        self.env.insert(
            "db_row_get_int".to_string(),
            Type::Function(vec![Type::Int, Type::Int], Box::new(Type::Int)),
        );
        // db_row_advance(result: Int) -> Int (1=more, 0=done)
        self.env.insert(
            "db_row_advance".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        // db_result_free(result: Int) -> Unit
        self.env.insert(
            "db_result_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
        // db_close(db: Int) -> Unit
        self.env.insert(
            "db_close".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
    }

    /// Registers HTTP server builtins.
    fn register_http_server_functions(&mut self) {
        // http_server_new(Int) -> Int
        self.env.insert(
            "http_server_new".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        // http_server_recv(Int) -> Int
        self.env.insert(
            "http_server_recv".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Int)),
        );
        // http_request_method(Int) -> String
        self.env.insert(
            "http_request_method".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::String)),
        );
        // http_request_path(Int) -> String
        self.env.insert(
            "http_request_path".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::String)),
        );
        // http_request_body(Int) -> String
        self.env.insert(
            "http_request_body".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::String)),
        );
        // http_respond(Int, Int, String) -> Unit
        self.env.insert(
            "http_respond".to_string(),
            Type::Function(
                vec![Type::Int, Type::Int, Type::String],
                Box::new(Type::Unit),
            ),
        );
        // http_server_free(Int) -> Unit
        self.env.insert(
            "http_server_free".to_string(),
            Type::Function(vec![Type::Int], Box::new(Type::Unit)),
        );
    }
}
