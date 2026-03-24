# tree-sitter-kodo

[Tree-sitter](https://tree-sitter.github.io/) grammar for the [Kodo programming language](https://github.com/kodo-lang/kodo).

## Usage

### Neovim (nvim-treesitter)

Add the parser to your nvim-treesitter config:

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

Then install with `:TSInstall kodo`.

### Development

```bash
npm install
npm run generate   # Generate parser from grammar.js
npm run test       # Run test corpus
```

## License

MIT
