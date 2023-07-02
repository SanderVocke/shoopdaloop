These text files list the dependency packages needed to build and/or run Shoopdaloop.
The filenames:
- ..._run_abstract.txt: lists generically what is needed (ends up in any created package dependency list). May include items which can have different providers (such as X, Jack).
- ..._run_default.txt:  specific list of packages which satisfies ..._run_abstract.txt and can be used to e.g. set up a build container.
- ..._build.txt:        what is needed to build ShoopDaLoop. Assumes ..._run_default.txt is already installed.