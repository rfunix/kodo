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
    /// Methods: `is_some`, `is_none`, `unwrap_or`.
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
    /// Methods: `is_ok`, `is_err`, `unwrap_or`.
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
                vec![list_ty.clone(), Type::Int, fn_acc_int_to_int],
                Box::new(Type::Int),
            ),
        );

        // List.count() -> Int
        self.method_lookup.insert(
            ("List".to_string(), "count".to_string()),
            ("List_count".to_string(), vec![list_ty.clone()], Type::Int),
        );
        self.env.insert(
            "List_count".to_string(),
            Type::Function(vec![list_ty.clone()], Box::new(Type::Int)),
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
            Type::Function(vec![ch_int], Box::new(Type::Int)),
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
    }
}
