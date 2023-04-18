call plug#begin()
Plug 'nvim-treesitter/nvim-treesitter', {'do': ':TSUpdate'}  " We recommend updating the parsers on update 
Plug 'pandark/42header.vim'
Plug 'nvim-telescope/telescope.nvim'
Plug 'nvim-lua/plenary.nvim'
Plug 'mbbill/undotree'
Plug 'zbirenbaum/copilot.lua'
Plug 'nathom/filetype.nvim'
Plug 'vim-scripts/argtextobj.vim'
Plug 'gruvbox-community/gruvbox'
Plug 'rose-pine/neovim'
" Plug 'tanvirtin/monokai.nvim'
Plug 'nvim-lualine/lualine.nvim'
Plug 'akinsho/bufferline.nvim'
Plug 'luochen1990/rainbow'
Plug 'phaazon/hop.nvim'
Plug 'chaoren/vim-wordmotion'
" Plug 'mirsella/nerdcommenter' " fork support for custom nerd-leaderkey (default = c )
" Plug 'tyru/caw.vim' " only one who work with vue and ↙
" Plug 'suy/vim-context-commentstring'
Plug 'JoosepAlviste/nvim-ts-context-commentstring'
Plug 'tpope/vim-commentary'
Plug 'Shougo/context_filetype.vim'
Plug 'mirsella/otherbufdo.nvim'
Plug 'tpope/vim-surround'
Plug 'tpope/vim-repeat'
Plug 'norcalli/nvim-colorizer.lua'
Plug 'machakann/vim-highlightedyank'
Plug 'terryma/vim-multiple-cursors'
Plug 'mattn/emmet-vim'
Plug 'svermeulen/vim-yoink'
Plug 'svermeulen/vim-cutlass'
Plug 'svermeulen/vim-subversive'
Plug 'tpope/vim-fugitive'
Plug 'Raimondi/delimitMate'
Plug 'nicwest/vim-camelsnek'
Plug 'AndrewRadev/splitjoin.vim'
Plug 'kyazdani42/nvim-web-devicons'
Plug 'simrat39/rust-tools.nvim'
Plug 'windwp/nvim-autopairs'
Plug 'lewis6991/impatient.nvim'
Plug 'leafOfTree/vim-vue-plugin'
Plug 'laytan/cloak.nvim'
Plug 'Eandrju/cellular-automaton.nvim'

Plug 'williamboman/mason.nvim'
Plug 'neovim/nvim-lspconfig'
Plug 'williamboman/mason-lspconfig.nvim'
Plug 'onsails/lspkind.nvim'
Plug 'folke/trouble.nvim'
Plug 'hrsh7th/cmp-nvim-lsp-signature-help'

Plug 'hrsh7th/vim-vsnip'
Plug 'rafamadriz/friendly-snippets'
Plug 'hrsh7th/cmp-nvim-lsp'
Plug 'hrsh7th/cmp-nvim-lua'
Plug 'zbirenbaum/copilot-cmp'
Plug 'hrsh7th/cmp-vsnip'
Plug 'hrsh7th/cmp-buffer'
Plug 'hrsh7th/cmp-path'
Plug 'hrsh7th/cmp-cmdline'
Plug 'petertriho/cmp-git'
Plug 'ray-x/cmp-treesitter'
Plug 'hrsh7th/nvim-cmp'
call plug#end()

command! W :execute ':silent w !sudo tee % > /dev/null' | :edit!
command! Cdfile :lcd %:p:h
map <Space> <Leader>
inoremap kj <Esc>
inoremap jj <Esc>:wa<cr>
map Y y$
nnoremap U :echo " < < ===== C H E C K   C A P S   L O C K ===== > > "<CR>
inoremap <C-H> <C-W>
nnoremap <M-F1> <nop>
xnoremap <M-F1> <nop>
inoremap <M-F1> <nop>
nnoremap <leader>. :noh<cr>

nnoremap <F1> :wa<cr>
inoremap <F1> <esc>:wa<cr>a
nnoremap <F2> :bw!
nnoremap <F3> :wa <bar> :bw<cr>
nnoremap <F4> :Telescope filetypes<cr>

nnoremap <F5> :vsplit<cr>
nnoremap <F6> :vert sb 

nnoremap <F7> :set wrap<cr>
nnoremap <F8> :set nowrap<cr>

" hop.nvim
nmap <leader>/ :HopPattern<cr>
nmap <Leader>f :HopChar1<cr>
nmap <leader>g :HopChar2<cr>
nmap <Leader>l :HopLine<cr>
nmap <Leader>w :HopWord<cr>
xmap <Leader>w :HopWord<cr>

nnoremap <leader>u :UndotreeToggle<cr>
nnoremap <leader>t :TroubleToggle<cr>
nnoremap <C-j> 5jzz
nnoremap <C-k> 5kzz
xnoremap <C-j> 5jzz
xnoremap <C-k> 5kzz
nnoremap <C-l> :bnext<CR>
" inoremap <C-l> <esc>:bnext<CR>
nnoremap <C-h> :bprev<CR>
" inoremap <C-h> <esc>:bprev<CR>
nnoremap <A-h> <C-w>h
nnoremap <A-j> <C-w>j
nnoremap <A-k> <C-w>k
nnoremap <A-l> <C-w>l
nnoremap <silent><leader>1 <Cmd>BufferLineGoToBuffer 1<CR>
nnoremap <silent><leader>2 <Cmd>BufferLineGoToBuffer 2<CR>
nnoremap <silent><leader>3 <Cmd>BufferLineGoToBuffer 3<CR>
nnoremap <silent><leader>4 <Cmd>BufferLineGoToBuffer 4<CR>
nnoremap <silent><leader>5 <Cmd>BufferLineGoToBuffer 5<CR>
nnoremap <silent><leader>6 <Cmd>BufferLineGoToBuffer 6<CR>
nnoremap <silent><leader>7 <Cmd>BufferLineGoToBuffer 7<CR>
nnoremap <silent><leader>8 <Cmd>BufferLineGoToBuffer 8<CR>
nnoremap <silent><leader>9 <Cmd>BufferLineGoToBuffer 9<CR>
nnoremap <silent><leader>$ <Cmd>BufferLineGoToBuffer -1<CR>

" plug yoink, cutlass & subversive
nmap <leader>n <plug>(YoinkPostPasteSwapBack)
nmap <leader>p <plug>(YoinkPostPasteSwapForward)
nmap p <plug>(YoinkPaste_p)
nmap P <plug>(YoinkPaste_P)
nmap [y <plug>(YoinkRotateBack)
nmap ]y <plug>(YoinkRotateForward)

nnoremap <leader>c c
xnoremap <leader>c c
nnoremap <leader>cc cc
nnoremap <leader>C C

nnoremap <leader>s s
xnoremap <leader>s s
nnoremap <leader>S S

nnoremap <leader>d d
xnoremap <leader>d d
nnoremap <leader>dd dd
nnoremap <leader>D D

nnoremap <leader>x x
nnoremap <leader>X X

nmap s <plug>(SubversiveSubstitute)
nmap ss <plug>(SubversiveSubstituteLine)
nmap S <plug>(SubversiveSubstituteToEndOfLine)

" filetype plugin indent on
set formatoptions-=cro " don't insert comment leader after o or O
" set guicursor = ""
set smartindent
set noexpandtab
set shiftwidth=4
set tabstop=4
set	softtabstop=4
set nowrap
set linebreak
set ignorecase " use \C in regex to search case sensitive
set noerrorbells
set hidden
" set noswapfile
set wildignorecase
set scrolloff=8
set history=1000
set mouse=a
set clipboard=unnamedplus
set lazyredraw
set updatetime=50
set splitbelow splitright
set incsearch
set termguicolors
set mousemoveevent
set completeopt=menu,menuone,preview,noselect
set number relativenumber


" themes
" colorscheme gruvbox
colorscheme rose-pine
" highlight Normal guibg=NONE
" highlight LineNr guifg=#7b6e63 guibg=NONE
" highlight CursorLineNr guifg=#f7bd2f guibg=NONE
" pink ↓
" highlight LineNr guifg=#f796ef guibg=NONE
" highlight CursorLineNr guifg=#f796ef guibg=NONE

" less mess
set backup
set undofile
" set backupdir=$XDG_STATE_HOME/vim/backup | call mkdir(&backupdir, 'p', 0700)
" set directory=$XDG_STATE_HOME/vim/swap   | call mkdir(&directory, 'p', 0700)
" set undodir=$XDG_STATE_HOME/vim/undo     | call mkdir(&undodir,   'p', 0700)
" set viewdir=$XDG_STATE_HOME/vim/view     | call mkdir(&viewdir,   'p', 0700)
let &backupdir = stdpath('state') .. '/nvim/backup' | call mkdir(&backupdir, 'p', 0700)
let &directory = stdpath('state') .. '/nvim/swap'   | call mkdir(&directory, 'p', 0700)
let &undodir   = stdpath('state') .. '/nvim/undo'   | call mkdir(&undodir,   'p', 0700)
let &viewdir   = stdpath('state') .. '/nvim/view'   | call mkdir(&viewdir,   'p', 0700)
let g:netrw_dirhistmax = 0

" yoink
let g:yoinkIncludeDeleteOperations=1
let g:yoinkSavePersistently=1
let g:yoinkAutoFormatPaste=1
let g:yoinkSwapClampAtEnds=1
let g:yoinkIncludeNamedRegisters=1
let g:yoinkSyncSystemClipboardOnFocus=1
let g:yoinkSavePersistently=1
let g:yoinkIncludeDeleteOperations=1

" highlightedyank
let g:highlightedyank_highlight_duration = 200

" fold settings
set foldmethod=expr
set foldexpr=nvim_treesitter#foldexpr()
set nofoldenable " disable folding at startup

" RainbowParentheses
let g:rainbow_active = 1

" -- Do not source the default filetype.vim for nathom/filetype.nvim
let g:did_load_filetypes = 1

" 42
let b:fortytwoheader_user = 'mirsella'
let b:fortytwoheader_mail = 'mirsella@protonmail.com'

" Find files using Telescope command-line sugar.
nnoremap <leader>ff <cmd>Telescope find_files<cr>
nnoremap <leader>fg <cmd>Telescope live_grep<cr>
nnoremap <leader>fb <cmd>Telescope buffers<cr>
nnoremap <leader>fh <cmd>Telescope help_tags<cr>

" vim-vsnip
" Expand
imap <expr> <C-j>   vsnip#expandable()  ? '<Plug>(vsnip-expand)'         : '<C-j>'
smap <expr> <C-j>   vsnip#expandable()  ? '<Plug>(vsnip-expand)'         : '<C-j>'
" Expand or jump
imap <expr> <C-l>   vsnip#available(1)  ? '<Plug>(vsnip-expand-or-jump)' : '<C-l>'
smap <expr> <C-l>   vsnip#available(1)  ? '<Plug>(vsnip-expand-or-jump)' : '<C-l>'
" Jump forward or backward
imap <expr> <Tab>   vsnip#jumpable(1)   ? '<Plug>(vsnip-jump-next)'      : '<Tab>'
smap <expr> <Tab>   vsnip#jumpable(1)   ? '<Plug>(vsnip-jump-next)'      : '<Tab>'
imap <expr> <S-Tab> vsnip#jumpable(-1)  ? '<Plug>(vsnip-jump-prev)'      : '<S-Tab>'
smap <expr> <S-Tab> vsnip#jumpable(-1)  ? '<Plug>(vsnip-jump-prev)'      : '<S-Tab>'

luafile ~/.config/nvim/lua-plugins.lua
