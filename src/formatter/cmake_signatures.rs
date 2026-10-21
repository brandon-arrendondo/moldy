// Argument signatures of CMake's built-in commands, written from the CMake
// command reference. A signature names the command's keywords by kind;
// arguments that are not keywords are positional. The CMake formatter
// (`cmake.rs`) uses a signature to group a call's arguments when the call
// is too long for one line, the way gersemi does. A command with no entry
// here is "unknown" and keeps its own layout.

/// Keywords of one command (or of one nested section of it).
pub(super) struct Signature {
    /// Flags that stand alone (`STATIC`, `REQUIRED`).
    pub options: &'static [&'static str],
    /// Keywords followed by exactly one value (`DESTINATION bin`).
    pub one_value: &'static [&'static str],
    /// Keywords followed by a list of values (`PRIVATE a b c`).
    pub multi_value: &'static [&'static str],
    /// Multi-value keywords whose values are name/value pairs (`PROPERTIES`).
    pub pairs: &'static [&'static str],
    /// Multi-value keywords whose values are a command line (`COMMAND`),
    /// packed as many to a line as fit.
    pub packed: &'static [&'static str],
    /// Multi-value keywords whose first value names the rest (`PROPERTY`).
    pub headed: &'static [&'static str],
    /// Keywords that open a nested section with its own signature.
    pub sections: &'static [(&'static str, &'static Signature)],
    /// The first argument stays on the opening line (`set(NAME`).
    pub front_inline: bool,
}

const fn sig(
    options: &'static [&'static str],
    one_value: &'static [&'static str],
    multi_value: &'static [&'static str],
) -> Signature {
    Signature {
        options,
        one_value,
        multi_value,
        pairs: &[],
        packed: &[],
        headed: &[],
        sections: &[],
        front_inline: false,
    }
}

impl Signature {
    /// Adds `COMMAND` as a command-line keyword.
    const fn with_command(self) -> Signature {
        Signature {
            packed: &["COMMAND"],
            ..self
        }
    }
}

/// All arguments positional: each gets its own line when expanded.
const POSITIONAL: Signature = sig(&[], &[], &[]);

const SCOPES: &[&str] = &["PRIVATE", "PUBLIC", "INTERFACE"];

const SET: Signature = Signature {
    options: &["PARENT_SCOPE", "FORCE"],
    one_value: &["CACHE"],
    multi_value: &[],
    pairs: &[],
    packed: &[],
    headed: &[],
    sections: &[],
    front_inline: true,
};

const TARGET_SCOPED: Signature = sig(&["BEFORE", "AFTER", "SYSTEM"], &[], SCOPES);

const INSTALL_ARTIFACT: Signature = sig(
    &[
        "OPTIONAL",
        "EXCLUDE_FROM_ALL",
        "NAMELINK_ONLY",
        "NAMELINK_SKIP",
    ],
    &["DESTINATION", "COMPONENT", "NAMELINK_COMPONENT"],
    &["PERMISSIONS", "CONFIGURATIONS"],
);

const INSTALL: Signature = Signature {
    options: &[
        "OPTIONAL",
        "EXCLUDE_FROM_ALL",
        "MESSAGE_NEVER",
        "USE_SOURCE_PERMISSIONS",
        "FILES_MATCHING",
        "EXPORT_LINK_INTERFACE_LIBRARIES",
        "ALL_COMPONENTS",
    ],
    one_value: &[
        "TYPE",
        "DESTINATION",
        "COMPONENT",
        "RENAME",
        "EXPORT",
        "NAMESPACE",
        "FILE",
        "PATTERN",
        "REGEX",
        "SCRIPT",
        "CODE",
        "EXPORT_NAME",
    ],
    multi_value: &[
        "TARGETS",
        "FILES",
        "PROGRAMS",
        "DIRECTORY",
        "PERMISSIONS",
        "CONFIGURATIONS",
        "FILE_PERMISSIONS",
        "DIRECTORY_PERMISSIONS",
        "RUNTIME_DEPENDENCIES",
        "IMPORTED_RUNTIME_ARTIFACTS",
    ],
    pairs: &[],
    packed: &[],
    headed: &[],
    sections: &[
        ("RUNTIME", &INSTALL_ARTIFACT),
        ("LIBRARY", &INSTALL_ARTIFACT),
        ("ARCHIVE", &INSTALL_ARTIFACT),
        ("OBJECTS", &INSTALL_ARTIFACT),
        ("FRAMEWORK", &INSTALL_ARTIFACT),
        ("BUNDLE", &INSTALL_ARTIFACT),
        ("PUBLIC_HEADER", &INSTALL_ARTIFACT),
        ("PRIVATE_HEADER", &INSTALL_ARTIFACT),
        ("RESOURCE", &INSTALL_ARTIFACT),
        ("FILE_SET", &INSTALL_ARTIFACT),
        ("CXX_MODULES_BMI", &INSTALL_ARTIFACT),
        ("INCLUDES", &INSTALL_ARTIFACT),
    ],
    front_inline: false,
};

const FIND_PATHS: &[&str] = &["NAMES", "HINTS", "PATHS", "PATH_SUFFIXES", "VALIDATOR"];

const FIND_OPTIONS: &[&str] = &[
    "NAMES_PER_DIR",
    "REQUIRED",
    "OPTIONAL",
    "NO_CACHE",
    "NO_DEFAULT_PATH",
    "NO_PACKAGE_ROOT_PATH",
    "NO_CMAKE_PATH",
    "NO_CMAKE_ENVIRONMENT_PATH",
    "NO_SYSTEM_ENVIRONMENT_PATH",
    "NO_CMAKE_SYSTEM_PATH",
    "NO_CMAKE_INSTALL_PREFIX",
    "CMAKE_FIND_ROOT_PATH_BOTH",
    "ONLY_CMAKE_FIND_ROOT_PATH",
    "NO_CMAKE_FIND_ROOT_PATH",
];

const FIND_ITEM: Signature = sig(FIND_OPTIONS, &["DOC", "ENV", "REGISTRY_VIEW"], FIND_PATHS);

const FIND_PACKAGE: Signature = sig(
    &[
        "EXACT",
        "QUIET",
        "REQUIRED",
        "MODULE",
        "CONFIG",
        "NO_MODULE",
        "NO_POLICY_SCOPE",
        "BYPASS_PROVIDER",
        "GLOBAL",
        "NO_DEFAULT_PATH",
        "NO_PACKAGE_ROOT_PATH",
        "NO_CMAKE_PATH",
        "NO_CMAKE_ENVIRONMENT_PATH",
        "NO_SYSTEM_ENVIRONMENT_PATH",
        "NO_CMAKE_PACKAGE_REGISTRY",
        "NO_CMAKE_SYSTEM_PATH",
        "NO_CMAKE_INSTALL_PREFIX",
        "NO_CMAKE_SYSTEM_PACKAGE_REGISTRY",
        "CMAKE_FIND_ROOT_PATH_BOTH",
        "ONLY_CMAKE_FIND_ROOT_PATH",
        "NO_CMAKE_FIND_ROOT_PATH",
    ],
    &["REGISTRY_VIEW"],
    &[
        "COMPONENTS",
        "OPTIONAL_COMPONENTS",
        "NAMES",
        "CONFIGS",
        "HINTS",
        "PATHS",
        "PATH_SUFFIXES",
    ],
);

const ADD_CUSTOM_COMMAND: Signature = sig(
    &[
        "PRE_BUILD",
        "PRE_LINK",
        "POST_BUILD",
        "VERBATIM",
        "APPEND",
        "USES_TERMINAL",
        "COMMAND_EXPAND_LISTS",
        "DEPENDS_EXPLICIT_ONLY",
        "CODEGEN",
    ],
    &[
        "TARGET",
        "MAIN_DEPENDENCY",
        "WORKING_DIRECTORY",
        "COMMENT",
        "DEPFILE",
        "JOB_POOL",
        "JOB_SERVER_AWARE",
    ],
    &["OUTPUT", "DEPENDS", "BYPRODUCTS", "IMPLICIT_DEPENDS"],
)
.with_command();

const ADD_CUSTOM_TARGET: Signature = sig(
    &["ALL", "VERBATIM", "USES_TERMINAL", "COMMAND_EXPAND_LISTS"],
    &[
        "WORKING_DIRECTORY",
        "COMMENT",
        "JOB_POOL",
        "JOB_SERVER_AWARE",
    ],
    &["DEPENDS", "BYPRODUCTS", "SOURCES"],
)
.with_command();

const EXECUTE_PROCESS: Signature = sig(
    &[
        "OUTPUT_QUIET",
        "ERROR_QUIET",
        "OUTPUT_STRIP_TRAILING_WHITESPACE",
        "ERROR_STRIP_TRAILING_WHITESPACE",
        "ECHO_OUTPUT_VARIABLE",
        "ECHO_ERROR_VARIABLE",
    ],
    &[
        "WORKING_DIRECTORY",
        "TIMEOUT",
        "RESULT_VARIABLE",
        "RESULTS_VARIABLE",
        "OUTPUT_VARIABLE",
        "ERROR_VARIABLE",
        "INPUT_FILE",
        "OUTPUT_FILE",
        "ERROR_FILE",
        "COMMAND_ECHO",
        "ENCODING",
        "COMMAND_ERROR_IS_FATAL",
    ],
    &[],
)
.with_command();

const PROPERTIES_ONLY: Signature = Signature {
    options: &[],
    one_value: &[],
    multi_value: &[],
    pairs: &["PROPERTIES"],
    packed: &[],
    headed: &[],
    sections: &[],
    front_inline: false,
};

const SET_SOURCE_FILES_PROPERTIES: Signature = Signature {
    options: &[],
    one_value: &[],
    multi_value: &["DIRECTORY", "TARGET_DIRECTORY"],
    pairs: &["PROPERTIES"],
    packed: &[],
    headed: &[],
    sections: &[],
    front_inline: false,
};

const SET_PROPERTY: Signature = Signature {
    headed: &["PROPERTY"],
    ..sig(
        &["GLOBAL", "APPEND", "APPEND_STRING"],
        &[],
        &["DIRECTORY", "TARGET", "SOURCE", "INSTALL", "TEST", "CACHE"],
    )
};

const GET_PROPERTY: Signature = sig(
    &[
        "GLOBAL",
        "VARIABLE",
        "SET",
        "DEFINED",
        "BRIEF_DOCS",
        "FULL_DOCS",
    ],
    &[
        "DIRECTORY",
        "TARGET",
        "SOURCE",
        "INSTALL",
        "TEST",
        "CACHE",
        "PROPERTY",
    ],
    &[],
);

const MESSAGE: Signature = sig(
    &[
        "FATAL_ERROR",
        "SEND_ERROR",
        "WARNING",
        "AUTHOR_WARNING",
        "DEPRECATION",
        "NOTICE",
        "STATUS",
        "VERBOSE",
        "DEBUG",
        "TRACE",
        "CHECK_START",
        "CHECK_PASS",
        "CHECK_FAIL",
        "CONFIGURE_LOG",
    ],
    &[],
    &[],
);

const FILE: Signature = sig(
    &[
        "NEWLINE_CONSUME",
        "NO_HEX_CONVERSION",
        "CONFIGURE_DEPENDS",
        "FOLLOW_SYMLINKS",
        "HEX",
        "UTC",
        "NO_SOURCE_PERMISSIONS",
        "USE_SOURCE_PERMISSIONS",
        "FILES_MATCHING",
        "SHOW_PROGRESS",
    ],
    &[
        "REGEX",
        "LIMIT_COUNT",
        "LIMIT_INPUT",
        "LIMIT_OUTPUT",
        "LENGTH_MINIMUM",
        "LENGTH_MAXIMUM",
        "ENCODING",
        "LIST_DIRECTORIES",
        "RELATIVE",
        "DESTINATION",
        "OFFSET",
        "LIMIT",
        "CONTENT",
        "CONDITION",
        "OUTPUT",
        "INPUT",
        "TARGET",
        "NEWLINE_STYLE",
        "RESULT",
        "RESULT_VARIABLE",
        "TIMEOUT",
        "INACTIVITY_TIMEOUT",
        "STATUS",
        "LOG",
        "EXPECTED_HASH",
        "EXPECTED_MD5",
        "TLS_VERIFY",
        "TLS_CAINFO",
        "USERPWD",
        "BASE_DIRECTORY",
    ],
    &[
        "PATTERN",
        "PERMISSIONS",
        "FILE_PERMISSIONS",
        "DIRECTORY_PERMISSIONS",
        "HTTPHEADER",
    ],
);

/// The signature of built-in command `name` (already lowercase).
pub(super) fn signature(name: &str) -> Option<&'static Signature> {
    const CMAKE_MINIMUM_REQUIRED: Signature = sig(&["FATAL_ERROR"], &["VERSION"], &[]);
    const PROJECT: Signature = sig(
        &[],
        &["VERSION", "DESCRIPTION", "HOMEPAGE_URL"],
        &["LANGUAGES"],
    );
    const UNSET: Signature = sig(&["CACHE", "PARENT_SCOPE"], &[], &[]);
    const ADD_EXECUTABLE: Signature = sig(
        &[
            "WIN32",
            "MACOSX_BUNDLE",
            "EXCLUDE_FROM_ALL",
            "IMPORTED",
            "GLOBAL",
        ],
        &["ALIAS"],
        &[],
    );
    const ADD_LIBRARY: Signature = sig(
        &[
            "STATIC",
            "SHARED",
            "MODULE",
            "OBJECT",
            "INTERFACE",
            "UNKNOWN",
            "EXCLUDE_FROM_ALL",
            "IMPORTED",
            "GLOBAL",
        ],
        &["ALIAS"],
        &[],
    );
    const TARGET_LINK_LIBRARIES: Signature = sig(
        &[],
        &[],
        &[
            "PRIVATE",
            "PUBLIC",
            "INTERFACE",
            "LINK_PRIVATE",
            "LINK_PUBLIC",
            "LINK_INTERFACE_LIBRARIES",
        ],
    );
    const TARGET_SOURCES: Signature = sig(
        &[],
        &["FILE_SET", "TYPE"],
        &["PRIVATE", "PUBLIC", "INTERFACE", "BASE_DIRS", "FILES"],
    );
    const INCLUDE: Signature = sig(&["OPTIONAL", "NO_POLICY_SCOPE"], &["RESULT_VARIABLE"], &[]);
    const ADD_SUBDIRECTORY: Signature = sig(&["EXCLUDE_FROM_ALL", "SYSTEM"], &[], &[]);
    const ADD_TEST: Signature = sig(
        &["COMMAND_EXPAND_LISTS"],
        &["NAME", "WORKING_DIRECTORY"],
        &["CONFIGURATIONS"],
    )
    .with_command();
    const CONFIGURE_FILE: Signature = sig(
        &[
            "COPYONLY",
            "ESCAPE_QUOTES",
            "@ONLY",
            "NO_SOURCE_PERMISSIONS",
            "USE_SOURCE_PERMISSIONS",
        ],
        &["NEWLINE_STYLE"],
        &["FILE_PERMISSIONS"],
    );
    const INCLUDE_DIRECTORIES: Signature = sig(&["AFTER", "BEFORE", "SYSTEM"], &[], &[]);
    const MARK_AS_ADVANCED: Signature = sig(&["CLEAR", "FORCE"], &[], &[]);
    const GET_FILENAME_COMPONENT: Signature = sig(&["CACHE"], &["BASE_DIR", "PROGRAM_ARGS"], &[]);
    const ENABLE_LANGUAGE: Signature = sig(&["OPTIONAL"], &[], &[]);
    const INCLUDE_GUARD: Signature = sig(&["DIRECTORY", "GLOBAL"], &[], &[]);
    const SOURCE_GROUP: Signature = sig(&[], &["TREE", "PREFIX", "REGULAR_EXPRESSION"], &["FILES"]);
    const CMAKE_PARSE_ARGUMENTS: Signature = sig(&[], &["PARSE_ARGV"], &[]);
    const RETURN: Signature = sig(&[], &[], &["PROPAGATE"]);
    const BLOCK: Signature = sig(&[], &[], &["SCOPE_FOR", "PROPAGATE"]);
    const EXPORT: Signature = sig(
        &["APPEND", "EXPORT_LINK_INTERFACE_LIBRARIES"],
        &[
            "EXPORT",
            "NAMESPACE",
            "FILE",
            "ANDROID_MK",
            "CXX_MODULES_DIRECTORY",
        ],
        &["TARGETS"],
    );
    const TRY_COMPILE: Signature = sig(
        &["NO_CACHE", "NO_LOG"],
        &[
            "OUTPUT_VARIABLE",
            "COPY_FILE",
            "COPY_FILE_ERROR",
            "LOG_DESCRIPTION",
            "C_STANDARD",
            "CXX_STANDARD",
            "PROJECT",
            "TARGET",
        ],
        &[
            "SOURCES",
            "SOURCE_FROM_CONTENT",
            "SOURCE_FROM_VAR",
            "SOURCE_FROM_FILE",
            "SOURCE_DIR",
            "BINARY_DIR",
            "CMAKE_FLAGS",
            "COMPILE_DEFINITIONS",
            "LINK_OPTIONS",
            "LINK_LIBRARIES",
        ],
    );

    const CHECK_SOURCE: Signature = sig(&[], &["SRC_EXT", "OUTPUT_VARIABLE"], &["FAIL_REGEX"]);
    const CHECK_LANGUAGE: Signature = sig(&["BUILTIN_TYPES_ONLY"], &["LANGUAGE"], &[]);
    const CHECK_IPO: Signature = sig(&[], &["RESULT", "OUTPUT"], &["LANGUAGES"]);
    const CHECK_PIE: Signature = sig(&[], &["OUTPUT_VARIABLE"], &["LANGUAGES"]);
    const PUSH_CHECK_STATE: Signature = sig(&["RESET"], &[], &[]);
    const FPHSA: Signature = sig(
        &[
            "CONFIG_MODE",
            "HANDLE_COMPONENTS",
            "NAME_MISMATCHED",
            "HANDLE_VERSION_RANGE",
        ],
        &[
            "FOUND_VAR",
            "VERSION_VAR",
            "REASON_FAILURE_MESSAGE",
            "FAIL_MESSAGE",
        ],
        &["REQUIRED_VARS"],
    );
    const FETCHCONTENT_DECLARE: Signature = sig(
        &["OVERRIDE_FIND_PACKAGE", "EXCLUDE_FROM_ALL", "SYSTEM"],
        &[
            "URL_HASH",
            "URL_MD5",
            "GIT_REPOSITORY",
            "GIT_TAG",
            "GIT_SHALLOW",
            "GIT_PROGRESS",
            "GIT_REMOTE_NAME",
            "SVN_REPOSITORY",
            "SVN_REVISION",
            "HG_REPOSITORY",
            "HG_TAG",
            "SOURCE_DIR",
            "BINARY_DIR",
            "SOURCE_SUBDIR",
            "DOWNLOAD_EXTRACT_TIMESTAMP",
            "DOWNLOAD_NO_EXTRACT",
            "TIMEOUT",
            "INACTIVITY_TIMEOUT",
        ],
        &[
            "URL",
            "FIND_PACKAGE_ARGS",
            "GIT_SUBMODULES",
            "PATCH_COMMAND",
            "UPDATE_COMMAND",
        ],
    );
    const PACKAGE_CONFIG_FILE: Signature = sig(
        &[
            "NO_SET_AND_CHECK_MACRO",
            "NO_CHECK_REQUIRED_COMPONENTS_MACRO",
        ],
        &["INSTALL_DESTINATION", "INSTALL_PREFIX"],
        &["PATH_VARS"],
    );
    const PACKAGE_VERSION_FILE: Signature =
        sig(&["ARCH_INDEPENDENT"], &["VERSION", "COMPATIBILITY"], &[]);
    const PKG_CHECK_MODULES: Signature = sig(
        &[
            "REQUIRED",
            "QUIET",
            "NO_CMAKE_PATH",
            "NO_CMAKE_ENVIRONMENT_PATH",
            "IMPORTED_TARGET",
            "GLOBAL",
        ],
        &[],
        &[],
    );
    const GTEST_TESTS: Signature = Signature {
        pairs: &["PROPERTIES"],
        ..sig(
            &["NO_PRETTY_TYPES", "NO_PRETTY_VALUES"],
            &[
                "WORKING_DIRECTORY",
                "TEST_PREFIX",
                "TEST_SUFFIX",
                "TEST_FILTER",
                "TEST_LIST",
                "DISCOVERY_TIMEOUT",
                "DISCOVERY_MODE",
                "XML_OUTPUT_DIR",
                "TARGET",
            ],
            &["EXTRA_ARGS", "SOURCES", "TEST_LIST", "SKIP_DEPENDENCY"],
        )
    };
    const SET_PACKAGE_PROPERTIES: Signature = Signature {
        pairs: &["PROPERTIES"],
        ..sig(&[], &["URL", "DESCRIPTION", "TYPE", "PURPOSE"], &[])
    };
    const FEATURE_SUMMARY: Signature = sig(
        &[
            "APPEND",
            "INCLUDE_QUIET_PACKAGES",
            "FATAL_ON_MISSING_REQUIRED_PACKAGES",
            "QUIET_ON_EMPTY",
        ],
        &["FILENAME", "VAR", "DESCRIPTION", "DEFAULT_DESCRIPTION"],
        &["WHAT"],
    );
    const GENERATE_EXPORT_HEADER: Signature = sig(
        &["DEFINE_NO_DEPRECATED"],
        &[
            "BASE_NAME",
            "EXPORT_MACRO_NAME",
            "EXPORT_FILE_NAME",
            "DEPRECATED_MACRO_NAME",
            "NO_EXPORT_MACRO_NAME",
            "INCLUDE_GUARD_NAME",
            "STATIC_DEFINE",
            "NO_DEPRECATED_MACRO_NAME",
            "PREFIX_NAME",
            "CUSTOM_CONTENT_FROM_VARIABLE",
        ],
        &[],
    );

    const HOST_SYSTEM_INFORMATION: Signature = sig(&[], &["RESULT"], &["QUERY"]);

    Some(match name {
        "check_c_source_compiles"
        | "check_cxx_source_compiles"
        | "check_c_source_runs"
        | "check_cxx_source_runs"
        | "check_source_compiles"
        | "check_source_runs"
        | "check_c_compiler_flag"
        | "check_cxx_compiler_flag"
        | "check_compiler_flag"
        | "check_linker_flag" => &CHECK_SOURCE,
        "check_type_size"
        | "check_struct_has_member"
        | "check_symbol_exists"
        | "check_cxx_symbol_exists"
        | "check_prototype_definition" => &CHECK_LANGUAGE,
        "check_include_file"
        | "check_include_files"
        | "check_include_file_cxx"
        | "check_function_exists"
        | "check_library_exists"
        | "check_variable_exists"
        | "check_fortran_function_exists"
        | "cmake_dependent_option"
        | "cmake_pop_check_state"
        | "cmake_reset_check_state"
        | "find_package_message"
        | "fetchcontent_makeavailable"
        | "fetchcontent_populate"
        | "fetchcontent_getproperties"
        | "add_feature_info"
        | "cmake_print_variables"
        | "write_compiler_detection_header" => &POSITIONAL,
        "check_ipo_supported" => &CHECK_IPO,
        "check_pie_supported" => &CHECK_PIE,
        "cmake_push_check_state" => &PUSH_CHECK_STATE,
        "find_package_handle_standard_args" => &FPHSA,
        "fetchcontent_declare" => &FETCHCONTENT_DECLARE,
        "configure_package_config_file" => &PACKAGE_CONFIG_FILE,
        "write_basic_package_version_file" => &PACKAGE_VERSION_FILE,
        "pkg_check_modules" | "pkg_search_module" => &PKG_CHECK_MODULES,
        "gtest_discover_tests" | "gtest_add_tests" => &GTEST_TESTS,
        "set_package_properties" => &SET_PACKAGE_PROPERTIES,
        "feature_summary" => &FEATURE_SUMMARY,
        "generate_export_header" => &GENERATE_EXPORT_HEADER,
        "set" => &SET,
        "unset" => &UNSET,
        "cmake_minimum_required" => &CMAKE_MINIMUM_REQUIRED,
        "project" => &PROJECT,
        "message" => &MESSAGE,
        "add_executable" => &ADD_EXECUTABLE,
        "add_library" => &ADD_LIBRARY,
        "target_link_libraries" => &TARGET_LINK_LIBRARIES,
        "target_include_directories"
        | "target_compile_definitions"
        | "target_compile_options"
        | "target_compile_features"
        | "target_link_options"
        | "target_link_directories"
        | "target_precompile_headers" => &TARGET_SCOPED,
        "target_sources" => &TARGET_SOURCES,
        "set_target_properties" | "set_tests_properties" | "set_directory_properties" => {
            &PROPERTIES_ONLY
        }
        "set_source_files_properties" => &SET_SOURCE_FILES_PROPERTIES,
        "set_property" => &SET_PROPERTY,
        "get_property" => &GET_PROPERTY,
        "find_package" => &FIND_PACKAGE,
        "find_library" | "find_path" | "find_program" | "find_file" => &FIND_ITEM,
        "include" => &INCLUDE,
        "add_subdirectory" => &ADD_SUBDIRECTORY,
        "install" => &INSTALL,
        "add_custom_command" => &ADD_CUSTOM_COMMAND,
        "add_custom_target" => &ADD_CUSTOM_TARGET,
        "add_test" => &ADD_TEST,
        "configure_file" => &CONFIGURE_FILE,
        "include_directories" => &INCLUDE_DIRECTORIES,
        "mark_as_advanced" => &MARK_AS_ADVANCED,
        "get_filename_component" => &GET_FILENAME_COMPONENT,
        "enable_language" => &ENABLE_LANGUAGE,
        "include_guard" => &INCLUDE_GUARD,
        "source_group" => &SOURCE_GROUP,
        "cmake_parse_arguments" => &CMAKE_PARSE_ARGUMENTS,
        "return" => &RETURN,
        "block" => &BLOCK,
        "export" => &EXPORT,
        "try_compile" => &TRY_COMPILE,
        "execute_process" => &EXECUTE_PROCESS,
        "cmake_host_system_information" => &HOST_SYSTEM_INFORMATION,
        "file" => &FILE,
        "option"
        | "list"
        | "string"
        | "math"
        | "add_dependencies"
        | "add_compile_options"
        | "add_compile_definitions"
        | "add_definitions"
        | "remove_definitions"
        | "add_link_options"
        | "link_directories"
        | "link_libraries"
        | "enable_testing"
        | "get_target_property"
        | "get_directory_property"
        | "get_source_file_property"
        | "get_test_property"
        | "get_cmake_property"
        | "define_property"
        | "separate_arguments"
        | "cmake_policy"
        | "cmake_path"
        | "cmake_language"
        | "variable_watch"
        | "site_name"
        | "aux_source_directory"
        | "foreach"
        | "endforeach"
        | "while"
        | "endwhile"
        | "if"
        | "elseif"
        | "else"
        | "endif"
        | "function"
        | "endfunction"
        | "macro"
        | "endmacro"
        | "endblock"
        | "break"
        | "continue"
        | "try_run"
        | "create_test_sourcelist"
        | "fltk_wrap_ui"
        | "qt_wrap_cpp"
        | "qt_wrap_ui"
        | "load_cache"
        | "ctest_build"
        | "ctest_test"
        | "exec_program"
        | "make_directory"
        | "remove"
        | "write_file"
        | "subdirs"
        | "install_files"
        | "install_programs"
        | "install_targets"
        | "output_required_files"
        | "use_mangled_mesa"
        | "utility_source"
        | "variable_requires"
        | "build_name"
        | "subdir_depends"
        | "export_library_dependencies" => &POSITIONAL,
        _ => return None,
    })
}

/// How many leading positional arguments of built-in command `name` are
/// fixed (a target or variable name, a subcommand) rather than part of its
/// value list. gersemi expands a call whose value list has more than four
/// entries.
pub(super) fn fixed_front(name: &str) -> usize {
    match name {
        "message"
        | "add_compile_options"
        | "add_compile_definitions"
        | "add_definitions"
        | "remove_definitions"
        | "add_link_options"
        | "include_directories"
        | "link_directories"
        | "link_libraries"
        | "mark_as_advanced" => 0,
        "list" | "string" | "file" | "cmake_path" | "cmake_language" | "math" => 2,
        _ => 1,
    }
}
