# The title bar's Git popup for the focused checkout, and its pull request.

git-menu-checking = Checking working tree...
git-menu-clean = No uncommitted changes
# Lines added and removed, as "+12 -3", and untracked files and folders.
git-menu-changes = { $untracked ->
        [0] +{ $additions } -{ $deletions }
        [one] +{ $additions } -{ $deletions }, { $untracked } untracked entry
       *[other] +{ $additions } -{ $deletions }, { $untracked } untracked entries
    }
git-menu-commit = Commit...
git-menu-push = Push
git-menu-open-pull-request = Open pull request #{ $number }
git-menu-create-pull-request = Create pull request
git-menu-checks-and-comments = Checks and comments
git-menu-comment = Comment...
git-menu-merge = Merge...
git-menu-not-committed = { $untracked ->
        [0] Not yet committed
       *[other] Not yet committed ({ $untracked } untracked)
    }
git-menu-draft = Draft — not ready for review
git-menu-ready = Ready for review
# Headings beside the pull request's review state, mergeability, and checks.
git-menu-review-heading = Review
git-menu-merge-heading = Merge
git-menu-checks-heading = Checks
git-menu-loading-pull-request = Loading pull request...
git-menu-open-in-browser = Open in browser
# Restacking the branch once the pull request below it in its GitHub stack
# merged. $base is the stack's base branch; $number the merged pull request;
# $branch the focused branch.
git-menu-restack = Restack onto { $base }...
git-menu-restack-title = Restack onto { $base }
git-menu-restack-body = #{ $number } merged below this branch, which still carries its old commits. Restacking replays only { $branch }'s own commits onto { $base } as the remote has it now. It changes this checkout only and pushes nothing; if commits conflict, the rebase stops for you to resolve.
git-menu-restack-confirm = Restack
# What a running Git action says while it runs.
git-menu-running-commit = Committing...
git-menu-running-push = Pushing...
git-menu-running-pull-request = Creating pull request...
git-menu-running-restack = Restacking...
# What a restack did. $remote and $base name where it now sits.
git-menu-restacked = Restacked { $branch } onto { $remote }/{ $base }; push it with --force-with-lease when ready.
git-menu-restack-unchanged = Nothing to restack: { $branch } no longer carries the merged commits.
git-menu-restack-changed = The branch or its stack changed since this dialog opened. Review it again.
