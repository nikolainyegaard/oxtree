# Cleanup actions

A cleanup is a shell command run on the selected item from the Clean Up menu. The set is read from `~/.config/oxtree/cleanups.conf` (or `$XDG_CONFIG_HOME/oxtree/cleanups.conf`). When that file does not exist, OxTree reads `~/.config/QDirStat/QDirStat-cleanup.conf` if present, since the format is the same; otherwise it writes the standard set below so there is a file to edit. There is no editor dialog yet; edit the file and restart.

## File format

One section per cleanup, numbered in menu order:

```
[Cleanup_01]
Active=true
AskForConfirmation=false
Command=%filemanager %d &
Hotkey=Ctrl+G
OutputWindowPolicy=ShowNever
Recurse=false
RefreshPolicy=NoRefresh
Shell=
Title=Open File &Manager Here
WorksForDir=true
WorksForDotEntry=true
WorksForFile=true
```

Values with quotes, backslashes or newlines are written in double quotes with backslash escapes, the way QSettings writes them.

| Key | Values | Meaning |
|-----|--------|---------|
| Command | shell text | Run with `shell -c`. Placeholders below are expanded first. |
| Title | text | Menu label. An `&` marks the accelerator letter and is not shown. |
| Active | true, false | Inactive cleanups are not listed. |
| WorksForDir, WorksForFile, WorksForDotEntry | true, false | Which item kinds enable the entry. `<Files>` is the dot entry. |
| AskForConfirmation | true, false | Show the confirmation dialog with the expanded command first. |
| RefreshPolicy | NoRefresh, RefreshThis, RefreshParent, AssumeDeleted | Anything other than NoRefresh rescans the tree when the command ends. |
| OutputWindowPolicy | ShowAlways, ShowIfErrorOutput, ShowAfterTimeout, ShowNever | When the output window opens. ShowAfterTimeout behaves like ShowIfErrorOutput for now. |
| Shell | path or empty | Shell for this cleanup. Empty means `$SHELL`, then `/bin/bash`, then `/bin/sh`. |
| Hotkey | `Ctrl+Shift+Delete` style | Keyboard shortcut while no text field has focus. |
| Recurse | true, false | Read and written, not acted on yet. |

## Placeholders

| Placeholder | Expands to |
|-------------|------------|
| `%p` | Full path of the item, single-quoted for the shell |
| `%n` | Name of the item, quoted |
| `%d` | Directory the command runs in: the item for directories, its parent for files, quoted |
| `%terminal` | The desktop's terminal: `konsole --workdir %d` on KDE, `gnome-terminal &` on GNOME, `xfce4-terminal &`, `lxterminal &`, `eterm &`, else `xterm` |
| `%filemanager` | The desktop's file manager: `xdg-open` on KDE, `nautilus`, `thunar`, `pcmanfm`, else `xdg-open` |

A single quote inside a name becomes `'\''`, the only quoting `sh` accepts.

## Standard set

| Title | Command | Works for | Confirm | Refresh |
|-------|---------|-----------|---------|---------|
| Open File Manager Here | `%filemanager %d &` | all | no | no |
| Open Terminal Here | `%terminal` | all | no | no |
| Check File Type | `file %n \| sed -e 's/[:,] /\n  /g'` | files | no | no |
| Compress | `cd ..; tar cjvf %n.tar.bz2 %n && rm -rf %n` | directories | no | parent |
| make clean | `make clean` | directories, `<Files>` | no | this |
| git clean | `git clean -dfx` | directories, `<Files>` | yes | this |
| Delete Junk Files | `rm -f *~ *.bak *.auto core` | directories, `<Files>` | no | this |
| Delete (no way to undelete!) | `rm -rf %p` | directories, files | yes | parent |
| Clear Directory Contents | `rm -rf %d/*` | directories | yes | this |

## Move to Trash

Not a cleanup: Edit, Move to Trash (Delete) runs `gio trash -- PATH` on a thread and rescans on success. `gio` is part of GLib and present on every desktop that has a trash.
