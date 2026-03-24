; Keywords
[
  "module" "meta" "fn" "let" "mut" "if" "else" "return"
  "while" "for" "match" "import" "from"
  "struct" "enum" "trait" "impl" "type" "pub"
  "async" "await" "spawn" "actor" "parallel" "channel"
  "own" "ref" "is" "in" "dyn" "self"
  "requires" "ensures" "invariant"
  "break" "continue"
  "test" "describe" "setup" "teardown" "forall"
  "intent"
] @keyword

; Operators
["+" "-" "*" "/" "%" "=" "==" "!=" "<" ">" "<=" ">=" "&&" "||" "!" "??" "?"] @operator
["->" "=>" "::" ".." "..="] @operator

; Delimiters
["(" ")" "{" "}" "[" "]"] @punctuation.bracket
["," ";" ":" "."] @punctuation.delimiter

; Booleans
(boolean_literal) @boolean

; Constructors
((identifier) @constant.builtin
 (#any-of? @constant.builtin "Some" "None" "Ok" "Err"))

; Built-in types
((identifier) @type.builtin
 (#any-of? @type.builtin
  "Int" "Int8" "Int16" "Int32" "Int64"
  "Uint" "Uint8" "Uint16" "Uint32" "Uint64"
  "Float" "Float32" "Float64"
  "Bool" "String" "Byte" "Unit" "Char"
  "Option" "Result" "List" "Map" "Set" "Channel" "Future"))

; Literals
(integer_literal) @number
(float_literal) @number.float
(string_literal) @string
(fstring_literal) @string
(interpolation) @string.special
(escape_sequence) @string.escape

; Comments
(line_comment) @comment
(block_comment) @comment

; Function definitions
(function_definition name: (identifier) @function)

; Function calls
(call_expression function: (identifier) @function.call)
(method_call_expression method: (identifier) @function.method.call)

; Module name
(module_declaration name: (identifier) @module)

; Type definitions
(struct_definition name: (identifier) @type)
(enum_definition name: (identifier) @type)
(trait_definition name: (identifier) @type)
(actor_definition name: (identifier) @type)
(type_alias name: (identifier) @type)
(enum_variant name: (identifier) @constant)

; Type annotations
(named_type (identifier) @type)
(generic_type (identifier) @type)

; Fields
(field_definition name: (identifier) @property)
(field_expression field: (identifier) @property)
(field_initializer name: (identifier) @property)

; Parameters
(parameter name: (identifier) @variable.parameter)
(closure_parameter name: (identifier) @variable.parameter)

; Annotations
(annotation (identifier) @attribute)

; Meta keys
(meta_key) @property

; Patterns
(wildcard_pattern) @variable.builtin
(variant_pattern variant: (identifier) @constant)

; Self
(self_expression) @variable.builtin

; Test names
(test_block name: (string_literal) @string.special)
(describe_block name: (string_literal) @string.special)

; Intent name
(intent_declaration name: (identifier) @type)

; Identifiers (fallback)
(identifier) @variable
