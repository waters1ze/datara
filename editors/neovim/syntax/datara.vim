" Vim syntax file
" Language: Datara (.dtr)
" Maintainer: Datara Language Project

if exists("b:current_syntax")
  finish
endif

" Keywords
syn keyword dataraTriad let mut val
syn keyword dataraDecl fn function class entity behavior component role packet using flow process task actor enum
syn keyword dataraControl if else while for in return match break continue decide when or with assert panic
syn keyword dataraModifier pub extern async await const view mutView mut_view
syn keyword dataraIO out err print println eprintln input
syn keyword dataraImport use import export as
syn keyword dataraBoolean true false nil null None Some Ok Err
syn keyword dataraSelf self this

" Types
syn keyword dataraType Int Int64 Int32 Int16 Int8 UInt UInt64 UInt32 UInt16 UInt8 USize ISize Float Float64 Float32 Bool String Str Byte Char Option Result List Map Set Array Packet View MutView
syn match dataraUserType "\<[A-Z][a-zA-Z0-9_]*\>"

" Operators
syn match dataraOperator "[-+*/%=<>!&|^~?:]"
syn match dataraPipe "|>"
syn match dataraArrow "->"
syn match dataraFatArrow "=>"

" Numbers
syn match dataraNumber "\<\d\+\(_\d\+\)*\>"
syn match dataraHex "\<0x[0-9a-fA-F_]\+\>"
syn match dataraBinary "\<0b[01_]\+\>"
syn match dataraFloat "\<\d\+\(_\d\+\)*\.\d\+\(_\d\+\)*\([eE][+-]\?\d\+\)\?\>"

" Strings & Characters
syn region dataraString start='"' end='"' skip='\\"' contains=dataraEscape,dataraInterpolation
syn match dataraEscape "\\\([nrt\\"0]\|x[0-9a-fA-F]\{2}\|u{[0-9a-fA-F]\+}\)" contained
syn match dataraInterpolation "{[a-zA-Z_][a-zA-Z0-9_.]*}" contained

" Comments
syn match dataraCommentDoc "///.*$"
syn match dataraComment "//.*$"
syn region dataraBlockComment start="/\*" end="\*/"

" Highlighting Links
hi def link dataraTriad StorageClass
hi def link dataraDecl Keyword
hi def link dataraControl Conditional
hi def link dataraModifier StorageClass
hi def link dataraIO Function
hi def link dataraImport Include
hi def link dataraBoolean Boolean
hi def link dataraSelf Special
hi def link dataraType Type
hi def link dataraUserType Structure
hi def link dataraOperator Operator
hi def link dataraPipe Operator
hi def link dataraArrow Operator
hi def link dataraFatArrow Operator
hi def link dataraNumber Number
hi def link dataraHex Number
hi def link dataraBinary Number
hi def link dataraFloat Float
hi def link dataraString String
hi def link dataraEscape SpecialChar
hi def link dataraInterpolation Identifier
hi def link dataraComment Comment
hi def link dataraCommentDoc SpecialComment
hi def link dataraBlockComment Comment

let b:current_syntax = "datara"
