# Cleaning up history with git rebase

`git rebase -i main` opens a list of your commits. Change `pick` to `squash` (or `fixup`) to fold a commit into the one above it, `reword` to edit a message, or reorder lines to reorder commits.

When a conflict stops the rebase, fix the files, `git add` them and run `git rebase --continue`; `git rebase --abort` returns to where you started.

Rebasing rewrites commits, so a branch that was already pushed needs a force push. Use `git push --force-with-lease`, which refuses if someone else pushed in the meantime. Never rebase a shared branch like main. `git reflog` finds commits again after a rebase went wrong.
