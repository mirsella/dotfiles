export ZVM_INIT_MODE=sourcing
export ZVM_LINE_INIT_MODE=$ZVM_MODE_INSERT
source "${ZDOTDIR}/.antidote/antidote.zsh"
export HISTSIZE=1000000
export SAVEHIST=1000000
export HISTFILE="$XDG_DATA_HOME"/zsh_history
export HISTCONTROL=ignoreboth
if type oh-my-posh > /dev/null; then
	eval "$(oh-my-posh init zsh --config ~/.config/zsh/lib/velvet.json)"
else
	export PROMPT="%B%F{219}[%n@%M %~]%(!.%F{196}#%f.$) %f%b"
	export RPROMPT="%B%F{225}%? %*%F%B"
fi
export CASE_INSENSITIVE=true
export HYPHEN_INSENSITIVE=true
export ENABLE_CORRECTION=true
export MENU_COMPLETE=true


# _fzf_compgen_path() { command fd -t f -HIL --color=always -E .cache -E .local -E .git -E run -E media -E coc -E plugged . $1 }
# _fzf_compgen_dir() { command fd -t d -HIL --color=always -E .cache -E .local -E .git -E run -E media -E coc -E plugged . $1 }
export FZF_DEFAULT_COMMAND='fd -t f -HLI --color=always -E node_modules -E .steam -E .cache -E .local -E .git -E run -E media -E sys -E proc'
export FZF_CTRL_T_COMMAND="$FZF_DEFAULT_COMMAND"
export FZF_DEFAULT_OPTS='--ansi --height=100 --preview="
if [ -d {} ]; then
  lsd -Ah {}
else 
  bat -pp --color=always --theme=\"Monokai Extended Origin\" {}
fi
"'
export FZF_CTRL_T_OPTS="$FZF_DEFAULT_OPTS"

export ZSH_SYSTEM_CLIPBOARD_METHOD='wlp'

export VIMV_RM="trash -rf"
# export FORGIT_IGNORE_PAGER='bat -l gitignore -pp --color=always --theme="Monokai Extended Origin"'
export FORGIT_COPY_CMD='wl-copy'
export forgit_log=fglo
export forgit_diff=fgd
export forgit_add=fga
export forgit_reset_head=fgrh
export forgit_ignore=fgi
export forgit_checkout_file=fgcf
export forgit_checkout_branch=fgcb
export forgit_branch_delete=fgbd
export forgit_checkout_tag=fgct
export forgit_checkout_commit=fgco
export forgit_revert_commit=fgrc
export forgit_clean=fgclean
export forgit_stash_show=fgss
export forgit_stash_push=fgsp
export forgit_cherry_pick=fgcp
export forgit_rebase=fgrb
export forgit_blame=fgbl
export forgit_fixup=fgfu

setopt share_history # share history between all sessions.
setopt histappend
# setopt prompt_subst
setopt auto_cd # cd by typing directory name if it's not a command
setopt correct_all # autocorrect commands
setopt correct # autocorrect commands
setopt auto_list # automatically list choices on ambiguous completion
setopt always_to_end
setopt interactive_comments
setopt long_list_jobs
setopt complete_in_word
setopt extendedglob
setopt nonomatch # allow extended globbing and if no match found is use it as a literal string. ex: git show HEAD^
unsetopt auto_menu # automatically use menu completion
unsetopt hist_verify
zstyle ':completion::complete:*' gain-privileges 1

# paste url without quote
autoload -Uz url-quote-magic
zle -N self-insert url-quote-magic
autoload -Uz bracketed-paste-magic
zle -N bracketed-paste bracketed-paste-magic

autoload -Uz compinit
ZSH_COMPDUMP=${ZSH_COMPDUMP:-${ZDOTDIR}/.zcompdump}
if [[ $ZSH_COMPDUMP(#qNmh-20) ]]; then
  compinit -C -d "$ZSH_COMPDUMP"
else
  compinit -i -d "$ZSH_COMPDUMP"; touch "$ZSH_COMPDUMP"
fi
{
  # compile .zcompdump
  if [[ -s "$ZSH_COMPDUMP" && (! -s "${ZSH_COMPDUMP}.zwc" || "$ZSH_COMPDUMP" -nt "${ZSH_COMPDUMP}.zwc") ]]; then
    zcompile "$ZSH_COMPDUMP"
  fi
} &!

## case insensitive path-completion
zstyle ':completion:*' matcher-list 'm:{a-zA-Z}={A-Za-z}'
zstyle ':completion:*' menu select

# zsh auto notify
export AUTO_NOTIFY_THRESHOLD=15
export AUTO_NOTIFY_TITLE="code: %exit_code, took %elapsed sec.\n"
export AUTO_NOTIFY_BODY="%command"
export AUTO_NOTIFY_EXPIRE_TIME=2000
AUTO_NOTIFY_IGNORE+=("less" "more" "man" "tig" "watch" "git commit" "top" "htop" "nano" "v" "nvim" "s nvim -p" "ssh" "chezmoi cd")

ZSH_AUTOSUGGEST_STRATEGY=(history completion)
ZSH_AUTOSUGGEST_BUFFER_MAX_SIZE=20
ZSH_AUTOSUGGEST_USE_ASYNC=1
ZSH_AUTOSUGGEST_MANUAL_REBIND=1
# ZSH_AUTOSUGGEST_COMPLETION_IGNORE='(man)*'

# https://github.com/zsh-users/zsh-syntax-highlighting/blob/master/docs/highlighters/
declare -A ZSH_HIGHLIGHT_STYLES
ZSH_HIGHLIGHT_HIGHLIGHTERS=(main brackets pattern regexp cursor root line)
# ZSH_HIGHLIGHT_STYLES[error]=fg=160,bold
# ZSH_HIGHLIGHT_STYLES[globbing]=fg=225,bold
# # ZSH_HIGHLIGHT_STYLES[history-expansion]=fg=225,bold
# ZSH_HIGHLIGHT_STYLES[path]=fg=#EFDCF9,underline
# # ZSH_HIGHLIGHT_STYLES[autodirectory]=$ZSH_HIGHLIGHT_STYLES[path]
# # ZSH_HIGHLIGHT_STYLES[bracket-error]=$ZSH_HIGHLIGHT_STYLES[error]
# # ZSH_HIGHLIGHT_STYLES[bracket-level-1]=fg=129,bold
# # ZSH_HIGHLIGHT_STYLES[bracket-level-2]=fg=147,bold
# # ZSH_HIGHLIGHT_STYLES[bracket-level-3]=fg=159,bold
# # ZSH_HIGHLIGHT_STYLES[bracket-level-4]=fg=193,bold
# # ZSH_HIGHLIGHT_STYLES[commandseparator]=002,bold
# # ZSH_HIGHLIGHT_STYLES[back-quoted-argument]=fg=159
# # ZSH_HIGHLIGHT_STYLES[back-quoted-argument-unclosed]=$ZSH_HIGHLIGHT_STYLES[error]
# # ZSH_HIGHLIGHT_STYLES[single-quoted-argument]=fg=195
# # ZSH_HIGHLIGHT_STYLES[single-quoted-argument-unclosed]=$ZSH_HIGHLIGHT_STYLES[error]
# # ZSH_HIGHLIGHT_STYLES[double-quoted-argument]=fg=194
# # ZSH_HIGHLIGHT_STYLES[double-quoted-argument-unclosed]=$ZSH_HIGHLIGHT_STYLES[error]
# ZSH_HIGHLIGHT_STYLES[redirection]=fg=201,bold
# ZSH_HIGHLIGHT_STYLES[assign]=fg=213,bold
# # ZSH_HIGHLIGHT_STYLES[comment]='fg=240'
#
#
# # https://github.com/dracula/dracula-theme
# ZSH_HIGHLIGHT_STYLES[comment]='fg=#6272A4'
# ## Constants
# ## Entitites
# ## Functions/methods
# ZSH_HIGHLIGHT_STYLES[alias]='fg=#50FA7B'
# ZSH_HIGHLIGHT_STYLES[suffix-alias]='fg=#50FA7B'
# ZSH_HIGHLIGHT_STYLES[global-alias]='fg=#50FA7B'
# ZSH_HIGHLIGHT_STYLES[function]='fg=#50FA7B'
# ZSH_HIGHLIGHT_STYLES[command]='fg=#50FA7B'
# ZSH_HIGHLIGHT_STYLES[precommand]='fg=#50FA7B,italic'
# ZSH_HIGHLIGHT_STYLES[autodirectory]='fg=#FFB86C,italic'
# ZSH_HIGHLIGHT_STYLES[single-hyphen-option]='fg=#FFB86C'
# ZSH_HIGHLIGHT_STYLES[double-hyphen-option]='fg=#FFB86C'
# ZSH_HIGHLIGHT_STYLES[back-quoted-argument]='fg=#BD93F9'
# ## Keywords
# ## Built ins
# ZSH_HIGHLIGHT_STYLES[builtin]='fg=#8BE9FD'
# ZSH_HIGHLIGHT_STYLES[reserved-word]='fg=#8BE9FD'
# ZSH_HIGHLIGHT_STYLES[hashed-command]='fg=#8BE9FD'
# ## Punctuation
# ZSH_HIGHLIGHT_STYLES[commandseparator]='fg=#FF79C6'
# ZSH_HIGHLIGHT_STYLES[command-substitution-delimiter]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[command-substitution-delimiter-unquoted]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[process-substitution-delimiter]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[back-quoted-argument-delimiter]='fg=#FF79C6'
# ZSH_HIGHLIGHT_STYLES[back-double-quoted-argument]='fg=#FF79C6'
# ZSH_HIGHLIGHT_STYLES[back-dollar-quoted-argument]='fg=#FF79C6'
# ## Serializable / Configuration Languages
# ## Storage
# ## Strings
# ZSH_HIGHLIGHT_STYLES[command-substitution-quoted]='fg=#F1FA8C'
# ZSH_HIGHLIGHT_STYLES[command-substitution-delimiter-quoted]='fg=#F1FA8C'
# ZSH_HIGHLIGHT_STYLES[single-quoted-argument]='fg=#F1FA8C'
# ZSH_HIGHLIGHT_STYLES[single-quoted-argument-unclosed]='fg=#FF5555'
# ZSH_HIGHLIGHT_STYLES[double-quoted-argument]='fg=#F1FA8C'
# ZSH_HIGHLIGHT_STYLES[double-quoted-argument-unclosed]='fg=#FF5555'
# ZSH_HIGHLIGHT_STYLES[rc-quote]='fg=#F1FA8C'
# ## Variables
# ZSH_HIGHLIGHT_STYLES[dollar-quoted-argument]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[dollar-quoted-argument-unclosed]='fg=#FF5555'
# ZSH_HIGHLIGHT_STYLES[dollar-double-quoted-argument]='fg=#F8F8F2'
# # ZSH_HIGHLIGHT_STYLES[assign]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[named-fd]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[numeric-fd]='fg=#F8F8F2'
# ## No category relevant in spec
# ZSH_HIGHLIGHT_STYLES[unknown-token]='fg=#FF5555'
# # ZSH_HIGHLIGHT_STYLES[path]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[path_pathseparator]='fg=#FF79C6'
# ZSH_HIGHLIGHT_STYLES[path_prefix]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[path_prefix_pathseparator]='fg=#FF79C6'
# # ZSH_HIGHLIGHT_STYLES[globbing]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[history-expansion]='fg=#BD93F9'
# #ZSH_HIGHLIGHT_STYLES[command-substitution]='fg=?'
# #ZSH_HIGHLIGHT_STYLES[command-substitution-unquoted]='fg=?'
# #ZSH_HIGHLIGHT_STYLES[process-substitution]='fg=?'
# #ZSH_HIGHLIGHT_STYLES[arithmetic-expansion]='fg=?'
# ZSH_HIGHLIGHT_STYLES[back-quoted-argument-unclosed]='fg=#FF5555'
# # ZSH_HIGHLIGHT_STYLES[redirection]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[arg0]='fg=#F8F8F2'
# ZSH_HIGHLIGHT_STYLES[default]='fg=#F8F8F2'
# # ZSH_HIGHLIGHT_STYLES[ursor]='standout'

antidote load "${ZDOTDIR}/lib/$(hostname)/plugins.txt"

# fzf-tab
disable-fzf-tab
zstyle ':fzf-tab:*' fzf-bindings 'tab:toggle'
bindkey '^[OP' toggle-fzf-tab # F1 key
zstyle ':fzf-tab:*' continuous-trigger '/'
zstyle ":fzf-tab:*" fzf-flags '--preview-window=right:100:wrap' '--height=100' '--ansi'
zstyle ':fzf-tab:complete:*' fzf-preview '
if [ -d $realpath ]; then
  lsd -Ah $realpath
else 
  bat -pp --color=always --theme="Monokai Extended Origin" $realpath
fi
'

eval "$(zoxide init --cmd cd zsh)"
