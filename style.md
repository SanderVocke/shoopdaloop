# Code Style Guidelines

## 1. Naming Conventions
- Use snake_case for function and variable names
- Use descriptive variable names in test cases
- Use consistent naming conventions for test functions (e.g., 'test_' prefix)

## 2. Code Structure
- Use consistent indentation (4 spaces)
- Group related operations into separate functions or blocks
- Separate concerns with different methods for parsing different data types
- Separate Wayland-related code into distinct files and components
- Use classes for message handling (e.g., _OutgoingMessage and _IncomingMessage)

## 3. Documentation
- Use docstrings for method descriptions
- Fix spelling in documentation
- Explicitly specify documentation dependencies

## 4. Error Handling
- Prefer expect() over unwrap() for error handling, using meaningful error messages
- Handle potential errors explicitly, including UTF-8 conversion errors
- Use try-except blocks for error handling and logging
- Implement proper logging for debugging Wayland-related issues

## 5. Performance
- Remove redundant inline specifiers

## 6. Security
- Fix potential use of null pointer
- Enable clang nullability checks

## 7. Dependency Management
- Use specific version numbers for dependencies
- Keep dependencies up-to-date
- Use Dependabot for automated dependency updates
- Use submodules for third-party dependencies

## 8. Build System
- Prefer standard CMake variables (e.g., CMAKE_CXX_STANDARD and CMAKE_C_STANDARD)
- Format meson.build files with muon

## 9. Code Quality
- Avoid narrowing casts through void
- Avoid return with void value
- Suppress warnings in newer versions of clang and clang-tidy
- Use bare boolean literals

## 10. Testing
- Use descriptive test names that indicate the functionality being tested
- Organize tests into logical groups or scenarios
- Include setup and teardown steps in test cases

## 11. Type Safety
- Use type annotations, specifying types for function parameters

## 12. Compatibility
- Use environment variables to configure Wayland display and Qt platform
- Implement a Wayland compositor to embed Carla windows into the UI

## 13. Formatting
- Use consistent indentation and formatting in QML files