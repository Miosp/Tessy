# Tessy
A fast and simple build tool, which aims to intelligently accelerate the way you build your projects

## ⚠️ This project is a work in progress

This project was born out of frustration with existing build tools.
I wanted to make something between a task runner (`make` or `just`) and a build tool (mostly `gradle`).
I always felt like `make` and `just` were too simple, while `gradle` was a bit complex when needing to do simple tasks,
while also being slow.

## Features I aim for
- Dependency-based task execution
- File change detection, so only tasks with changed dependencies are executed
- Built-in support for basic OS-tasks like file manipulation
- Wrapper support, so you do not have to install Tessy globally to use it
- Easy installer
- Cross-platform support
