# Contributing

First, thank you for taking time out of your day to contribute to Xuehua!

This project is in its *very* early stages, so many basic things such as:
- Testing
- Documentation
- Guidelines
- And of course, code
are most likely incomplete or outdated, so all contributions are needed and welcome.

## Questions

If there are *any* questions you have about Xuehua, feel free to contact me over Discord (celestialexe).

## Commits

Once you know that the contribution fits within Xuehua:
- You may open a [pull request](https://github.com/CelestialCrafter/xuehua/pulls), and implement the contribution.
- If you can't or don't want to implement the contribution yourself, open an issue and a maintainer will implement the contribution whenever they have the time.

> [!NOTE]
> If you decide to make a pull request, consider [allowing maintainers to edit your pull request's changes](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/allowing-changes-to-a-pull-request-branch-created-from-a-fork) if you're comfortable with it.
> This allows maintainers to change stylistic choices or fix nitpicks in your pull request.

### Bug Reports

If you've found a bug, please report it by directly contacting a maintainer,
or creating an [issue on GitHub](https://github.com/CelestialCrafter/xuehua/issues).
In your report, please include:
- The issue (obviously)
- Your build's commit hash
- Reproduction steps
- Logs & Stack Traces
- Operating System & Platform
- Any other relevant context

### Enhancements

If you want to add new feature or enhancement to xuehua, please first discuss the enhancement with a maintainer on discord, GitHub, or anything else.
Please do not make pull requests for features without first talking to a maintainer.
Putting in time and effort into a contribution, just for it to not fit in the project is bad for everyone involved.

### Requirements

- Be kind to the maintainers
- Use rustfmt/stylua/relevant formatter on your code
- **(Maintainers)** Don't touch the wip branch, it's mine

### Messages

Commit messages are based on [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), but are relaxed for now, while the project is in its early stages.

All I require is:
```
(<crate|global>/<subsystem>) <description>

[detailed description]
```

Example:
```
(engine/builder) re-implement build scheduler to allow for concurrent builds

Removes the old synchronous builder, and implements a new asynchronous wave-based scheduler
Rough benchmarks show a 20-30% improvement in build speeds, and can potentially be optimised further
```

Thank you for helping Xuehua grow! <3
