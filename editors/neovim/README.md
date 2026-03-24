# Kodo for Neovim

Editor support for the [Kodo programming language](https://github.com/kodo-lang/kodo) in Neovim.

## Features

- Filetype detection for `.ko` files
- Syntax highlighting (Vim regex and tree-sitter queries)
- Indentation rules
- LSP integration (diagnostics, completion, hover, go-to-definition, rename, code actions)

## Installation

### Plugin (syntax, indent, ftdetect)

**lazy.nvim:**

```lua
{ "kodo-lang/kodo", dir = "editors/neovim" }
```

Or if published as a standalone repo:

```lua
{ "kodo-lang/kodo.vim" }
```

**vim-plug:**

```vim
Plug 'kodo-lang/kodo.vim'
```

### LSP Setup

The Kodo compiler includes a built-in LSP server. Make sure `kodoc` is in your `$PATH`.

#### Neovim 0.11+ (native LSP config)

```lua
vim.lsp.config.kodo = {
  cmd = { 'kodoc', 'lsp' },
  filetypes = { 'kodo' },
  root_markers = { '.git' },
}
vim.lsp.enable('kodo')
```

#### nvim-lspconfig

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.kodo then
  configs.kodo = {
    default_config = {
      cmd = { 'kodoc', 'lsp' },
      filetypes = { 'kodo' },
      root_dir = lspconfig.util.root_pattern('.git', '.'),
    },
  }
end

lspconfig.kodo.setup({})
```

#### LazyVim

LazyVim manages LSP servers through Mason. Since Kodo is not in the Mason registry,
you need to call `setup()` directly:

```lua
-- lua/plugins/kodo.lua
return {
  {
    "LazyVim/LazyVim",
    init = function()
      vim.filetype.add({ extension = { ko = "kodo" } })
    end,
  },
  {
    "neovim/nvim-lspconfig",
    dependencies = { "saghen/blink.cmp" },
    opts = function(_, opts)
      local lspconfig = require("lspconfig")
      local configs = require("lspconfig.configs")

      if not configs.kodo then
        configs.kodo = {
          default_config = {
            cmd = { "kodoc", "lsp" },
            filetypes = { "kodo" },
            root_dir = lspconfig.util.root_pattern(".git", "."),
          },
        }
      end

      local capabilities = vim.tbl_deep_extend(
        "force",
        vim.lsp.protocol.make_client_capabilities(),
        require("blink.cmp").get_lsp_capabilities()
      )
      lspconfig.kodo.setup({ capabilities = capabilities })
    end,
  },
}
```

> **Note:** If your LazyVim setup uses `nvim-cmp` instead of `blink.cmp`, replace the
> capabilities line with:
> ```lua
> local capabilities = require("cmp_nvim_lsp").default_capabilities()
> ```

### Tree-sitter (optional)

If a tree-sitter grammar for Kodo is available, add it to nvim-treesitter:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.kodo = {
  install_info = {
    url = "https://github.com/kodo-lang/tree-sitter-kodo",
    files = { "src/parser.c" },
    branch = "main",
  },
  filetype = "kodo",
}
```

Then run `:TSInstall kodo`.

## LSP Capabilities

The Kodo LSP server provides:

| Feature | Support |
|---------|---------|
| Diagnostics | Real-time error and warning reporting |
| Completion | Functions, types, keywords, methods, builtins |
| Hover | Type info, documentation, contracts |
| Go to Definition | Jump to symbol definitions |
| References | Find all references to a symbol |
| Document Symbols | Outline of module contents |
| Workspace Symbols | Search symbols across files |
| Rename | Rename symbols across the module |
| Code Actions | Quick fixes based on diagnostics |
| Signature Help | Parameter hints on function calls |

## Verify

Open a `.ko` file and run `:LspInfo` — you should see `kodo` listed as an active client.
