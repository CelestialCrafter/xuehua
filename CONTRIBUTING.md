# Contributing

This project is in its early stages, so many things such as:
- Testing
- Documentation
- Guidelines
- Core features

Are likely incomplete or outdated, so any contributions are needed and welcome.

## Enhancements

If you want to add a new feature or enhancement to Xuehua, you may open an [issue](https://github.com/CelestialCrafter/xuehua/issues) or [pull request](https://github.com/CelestialCrafter/xuehua/pulls).

> [!NOTE]
> If you decide to make a pull request, consider [allowing maintainers to edit your pull request's changes](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/allowing-changes-to-a-pull-request-branch-created-from-a-fork).
> This allows maintainers to change stylistic choices or fix nitpicks in your pull request.

### Bug Reports

If you've found a bug, please report it by directly contacting a maintainer,
or [creating an issue](https://github.com/CelestialCrafter/xuehua/issues).
In your report, please include:
- The issue
- Your build's commit hash
- Steps to reproduce
- Logs & Stack Traces
- Operating System/Platform
- Any other relevant context


### Commits

Commit messages are based on [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/), but are slightly modified and relaxed.

**Format:**
```
(<global|subsystem>) <description>

[more detailed description]
```

**Example:**
```
(engine/builder) re-implement build scheduler to allow for concurrent builds

Removes the old synchronous builder, and implements a new asynchronous wave-based scheduler
Rough benchmarks show a 20-30% improvement in build speeds, and can potentially be optimised further
```

Thank you for helping Xuehua! <3
