" Vim syntax file
" Language:   Globw (https://globw.org)
" Filenames:  *.gw
" Installed by the .deb and .rpm packages in /usr/share/vim/vimfiles/.

if exists("b:current_syntax")
  finish
endif

syn case match

syn keyword globwKeyword   lazy import break continue in if else while for from to step repeat return
syn keyword globwKeyword   class new is try catch always fail gives const match when enum global
syn keyword globwFunction  function procedure
syn match   globwKeyword   "\<end\s\+\(if\|loop\|function\|procedure\|class\|try\|match\|enum\)\>"
syn match   globwDecl      "\<variables\s*:"
syn keyword globwOperator  and or not div mod
syn keyword globwBoolean   true false none
syn keyword globwSelf      self parent
syn keyword globwType      int float string bool as
syn match   globwType      "\<\(list\|dict\|set\|tuple\|nungw\)\s\+of\>"
syn match   globwNumber    "\<\d\+\(\.\d\+\)\=\>"
syn match   globwCall      "\<\h\w*\ze\s*("
syn match   globwDefName   "\(\<\(function\|procedure\)\s\+\)\@<=\h\w*"
syn match   globwEscape    +\\[n"\\$]+ contained
syn match   globwTemplate  "\${[^}]*}" contained
syn region  globwString    start=+"+ skip=+\\\\\|\\"+ end=+"+ end=+$+ contains=globwEscape,globwTemplate
syn match   globwComment   "'.*$" contains=@Spell

hi def link globwKeyword   Statement
hi def link globwFunction  Statement
hi def link globwDecl      Statement
hi def link globwOperator  Operator
hi def link globwBoolean   Boolean
hi def link globwSelf      Special
hi def link globwType      Type
hi def link globwNumber    Number
hi def link globwCall      Function
hi def link globwDefName   Function
hi def link globwEscape    SpecialChar
hi def link globwTemplate  Special
hi def link globwString    String
hi def link globwComment   Comment

let b:current_syntax = "globw"
