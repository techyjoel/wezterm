
# Development Guide

## Git Usage

---

### 1. Branch Topology at a Glance

```
upstream/main  ──► upstream-sync ──► our/main ──► feature/*
        (WezTerm)      (temp)          (stable)     (work)
```

* **`upstream/main`** — remote-tracking branch pointing at the canonical WezTerm repository.  
* **`upstream-sync`** — throw-away branch recreated for every upstream pull; here we drop or tweak commits.  
* **`main`** — our tested integration branch; collaborators base their work here.  
* **`feature/*`** — short-lived topic branches for local development.

---

### 2. Day-to-Day Feature Work

~~~bash
git checkout -b feature/my-idea main
# …code, commit, test…
git rebase main               # or merge if you prefer
git checkout main
git merge feature/my-idea
git branch -d feature/my-idea
~~~

Because `main` never rewrites history, collaborators (and CI) can simply `git pull` and stay fast-forward.

---

### 3. Regular Upstream-Sync Workflow for Our WezTerm Fork

**Intent:** Pull in changes from upstream Wezterm as needed and desired. Doing this more often will reduce the pain (fewer changes to work through).

#### 3.1 Fetch upstream and start a fresh integration branch

~~~bash
git fetch upstream
git checkout -B upstream-sync upstream/main   # -B recreates the branch every time
~~~

`upstream-sync` is now byte-for-byte identical to the newest upstream commit.

#### 3.2 Review the new commits

Run an **interactive rebase** against the point we last accepted (`main`):

~~~bash
git rebase -i main
~~~

* Replace `pick` with `drop` to **omit** a commit entirely.  
* Replace `pick` with `edit` to **pause**, patch, and `git commit --amend`.  
* Leave `pick` unchanged to accept as-is.

If you later find a commit causes trouble, explicitly revert it:

~~~bash
git revert <sha>    # adds a “revert” commit
~~~

#### 3.3 Tag what you just reviewed

~~~bash
git tag -a upstream-merge-YYYY-MM-DD -m "Upstream through wez/wezterm@abcd1234"
~~~

On the next sync you can see what’s new with:

~~~bash
git log upstream-merge-YYYY-MM-DD..upstream/main --oneline
~~~

#### 3.4 Merge into `main`

~~~bash
git checkout main
git merge --no-ff upstream-sync      # keep a merge bubble documenting the range
~~~

Example merge message:

```
Merge upstream wez/wezterm (abcd1234..f00dbabe)
  • skipped: cafedead – experimental X11 change
  • modified: beefcafe – patched for Windows build
```

#### 3.5 Clean up

~~~bash
git branch -D upstream-sync   # always a temporary branch
~~~

#### 3.6. Why This Flow Works

| Goal                                    | How the workflow helps                                     |
|-----------------------------------------|------------------------------------------------------------|
| **Drop or tweak upstream commits**      | Interactive rebase in `upstream-sync` before merge.        |
| **Document what we processed**          | Annotated tag `upstream-merge-YYYY-MM-DD`.                 |
| **Stable base for collaborators/CI**    | `main` only receives tested merge bubbles.                 |
| **Minimal noise in history**            | One merge per upstream batch; feature branches squashable. |
| **Clear conflict-resolution surface**   | Conflicts handled once in `upstream-sync`, not in flight.  |

---

### 4. Build and Test

To build and test you should run the cargo build command in release mode, then you can either directly run the compiled executable in the target/release folder, or you can create a deployment package for your target.

~~~bash
cargo build --release
ci/deploy.sh
~~~

To install deployment packages:

* MacOS: Drag the .app into the Applications folder
* Windows: The default build is a standalone "zip drive" build. You can run the executable directly from that folder. ToDo: figure out how to build the windows installer and execute that
* Ubuntu Linux: dpkg -i <generated .dpkg file>