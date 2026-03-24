if exists("b:did_indent")
  finish
endif
let b:did_indent = 1

setlocal indentexpr=KodoIndent()
setlocal indentkeys=0{,0},0),0],!^F,o,O,e
setlocal autoindent

function! KodoIndent()
  let lnum = prevnonblank(v:lnum - 1)
  if lnum == 0
    return 0
  endif

  let prev = getline(lnum)
  let cur = getline(v:lnum)
  let ind = indent(lnum)

  " Increase indent after lines ending with { ( [
  if prev =~ '[{(\[]\s*$'
    let ind += shiftwidth()
  endif

  " Decrease indent for lines starting with } ) ]
  if cur =~ '^\s*[})\]]'
    let ind -= shiftwidth()
  endif

  return ind
endfunction
