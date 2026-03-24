if exists("b:current_syntax")
  finish
endif

" Keywords
syn keyword kodoKeyword module meta fn let mut if else return while for match import
syn keyword kodoKeyword struct enum trait impl self intent type pub
syn keyword kodoKeyword async await spawn actor parallel channel
syn keyword kodoKeyword own ref is in from dyn
syn keyword kodoKeyword requires ensures invariant
syn keyword kodoKeyword break continue
syn keyword kodoKeyword test describe setup teardown forall

" Booleans
syn keyword kodoBool true false

" Built-in enum constructors
syn keyword kodoConstructor Some None Ok Err

" Types
syn keyword kodoType Int Int8 Int16 Int32 Int64
syn keyword kodoType Uint Uint8 Uint16 Uint32 Uint64
syn keyword kodoType Float32 Float64 Float
syn keyword kodoType Bool String Byte Unit Char
syn keyword kodoType Option Result List Map Set Channel Future

" Annotations (@authored_by, @confidence, etc.)
syn match kodoAnnotation /@\w\+/

" Numbers
syn match kodoNumber /\<[0-9][0-9_]*\>/
syn match kodoFloat /\<[0-9][0-9_]*\.[0-9][0-9_]*\>/

" F-strings: f"hello {name}"
syn region kodoFString start=/f"/ end=/"/ contains=kodoFStringExpr,kodoEscape
syn region kodoFStringExpr start=/{/ end=/}/ contained

" Strings
syn region kodoString start=/"/ end=/"/ contains=kodoEscape
syn match kodoEscape /\\[nrt0"\\]/ contained

" Comments
syn match kodoComment /\/\/.*/
syn region kodoBlockComment start=/\/\*/ end=/\*\//

" Operators
syn match kodoOperator /[+\-\*\/=<>!&|%]/
syn match kodoArrow /->/
syn match kodoFatArrow /=>/
syn match kodoRange /\.\.\(=\)\?/
syn match kodoDoubleColon /::/
syn match kodoTryOp /?\ze[^?]/
syn match kodoNullCoalesce /??/
syn match kodoOptChain /?\./

" Meta keys (purpose:, version:, author:)
syn match kodoMetaKey /\<\(purpose\|version\|author\)\>\ze\s*:/

" Function names
syn match kodoFuncDef /\<fn\>\s\+\zs\w\+/

" Module name
syn match kodoModuleName /\<module\>\s\+\zs\w\+/

" Intent name
syn match kodoIntentName /\<intent\>\s\+\zs\w\+/

" Struct/Enum/Trait/Actor names
syn match kodoTypeDef /\<struct\>\s\+\zs\w\+/
syn match kodoTypeDef /\<enum\>\s\+\zs\w\+/
syn match kodoTypeDef /\<trait\>\s\+\zs\w\+/
syn match kodoTypeDef /\<actor\>\s\+\zs\w\+/

" Highlights
hi def link kodoKeyword Keyword
hi def link kodoBool Boolean
hi def link kodoConstructor Constant
hi def link kodoType Type
hi def link kodoAnnotation PreProc
hi def link kodoNumber Number
hi def link kodoFloat Float
hi def link kodoString String
hi def link kodoFString String
hi def link kodoFStringExpr Special
hi def link kodoEscape SpecialChar
hi def link kodoComment Comment
hi def link kodoBlockComment Comment
hi def link kodoOperator Operator
hi def link kodoArrow Operator
hi def link kodoFatArrow Operator
hi def link kodoRange Operator
hi def link kodoDoubleColon Operator
hi def link kodoTryOp Operator
hi def link kodoNullCoalesce Operator
hi def link kodoOptChain Operator
hi def link kodoMetaKey Identifier
hi def link kodoFuncDef Function
hi def link kodoModuleName Structure
hi def link kodoIntentName Structure
hi def link kodoTypeDef Structure

let b:current_syntax = "kodo"
