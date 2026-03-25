if exists("b:did_ftplugin")
  finish
endif
let b:did_ftplugin = 1

setlocal commentstring=//\ %s
setlocal comments=://
setlocal tabstop=4
setlocal shiftwidth=4
setlocal expandtab
setlocal smartindent

" Format on save via LSP (if an LSP client is attached)
augroup kodo_format_on_save
  autocmd! * <buffer>
  autocmd BufWritePre <buffer> lua vim.lsp.buf.format({ async = false })
augroup END
