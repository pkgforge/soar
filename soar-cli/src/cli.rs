use clap::{ArgAction, Parser, Subcommand, ValueHint};

use crate::utils::parse_default_repos_arg;

#[derive(Parser)]
#[command(
    author,
    version,
    about,
    help_template = "{before-help}{name} {version}
{author-with-newline}{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}",
    arg_required_else_help = true
)]
pub struct Args {
    /// Set output verbosity
    #[arg(short = 'v', long, action = ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Suppress outputs
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Output as json
    #[arg(short, long, global = true)]
    pub json: bool,

    /// Disable colors in output
    #[arg(long, global = true)]
    pub no_color: bool,

    /// Set current profile
    #[arg(short, long, global = true)]
    pub profile: Option<String>,

    /// Provide custom config file
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    /// Set proxy
    #[arg(required = false, long, short = 'P', global = true)]
    pub proxy: Option<String>,

    /// Set request headers
    #[arg(required = false, long, short = 'H', global = true)]
    pub header: Option<Vec<String>>,

    /// Set user agent
    #[arg(required = false, long, short = 'A', global = true)]
    pub user_agent: Option<String>,

    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum SelfAction {
    /// Update soar
    Update,
    /// Uninstall soar
    Uninstall,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Print the configuration file to stdout
    Config {
        /// Open the configuration file in editor
        /// Optional value can be passed to set as editor (default is $EDITOR)
        #[arg(required = false, short, long)]
        edit: Option<Option<String>>,
    },

    /// Install packages
    #[command(arg_required_else_help = true)]
    #[clap(name = "install", visible_alias = "i", visible_alias = "add")]
    Install {
        /// Packages to install
        #[arg(required = true)]
        packages: Vec<String>,

        /// Whether to force install the package
        #[arg(required = false, short, long)]
        force: bool,

        /// Skip all prompts and use first
        #[arg(required = false, short, long)]
        yes: bool,

        /// Set portable dir for home & config
        #[arg(required = false, long, num_args = 0..=1, value_hint = ValueHint::AnyPath)]
        portable: Option<Option<String>>,

        /// Set portable home
        #[arg(required = false, long, num_args = 0..=1, value_hint = ValueHint::AnyPath)]
        portable_home: Option<Option<String>>,

        /// Set portable config
        #[arg(required = false, long, num_args = 0..=1, value_hint = ValueHint::AnyPath)]
        portable_config: Option<Option<String>>,

        /// Set portable share
        #[arg(required = false, long, num_args = 0..=1, value_hint = ValueHint::AnyPath)]
        portable_share: Option<Option<String>>,

        /// Don't display notes
        #[arg(required = false, long)]
        no_notes: bool,

        /// Exclude log, build/spec files, and desktop integration files
        ///
        /// Note: This won't prevent desktop integration
        #[arg(required = false, long)]
        binary_only: bool,

        /// Ask for confirmation before installation
        #[arg(required = false, long, short)]
        ask: bool,
    },

    /// Search package
    #[command(arg_required_else_help = true)]
    #[clap(name = "search", visible_alias = "s", visible_alias = "find")]
    Search {
        /// Query to search
        #[arg(required = true)]
        query: String,

        /// Case sensitive search
        #[arg(required = false, long, alias = "exact")]
        case_sensitive: bool,

        /// Limit number of result
        #[arg(required = false, long)]
        limit: Option<usize>,
    },

    /// Query package info
    #[command(arg_required_else_help = true)]
    #[clap(name = "query", visible_alias = "Q")]
    Query {
        /// Package to query
        #[arg(required = true)]
        query: String,
    },

    /// Remove packages
    #[command(arg_required_else_help = true)]
    #[clap(name = "remove", visible_alias = "r", visible_alias = "del")]
    Remove {
        /// Packages to remove
        #[arg(required = true)]
        packages: Vec<String>,
    },

    /// Sync with remote metadata
    #[clap(name = "sync", visible_alias = "S", visible_alias = "fetch")]
    Sync,

    /// Update packages
    #[clap(name = "update", visible_alias = "u", visible_alias = "upgrade")]
    Update {
        /// Packages to update
        #[arg(required = false)]
        packages: Option<Vec<String>>,

        /// Keep old version
        #[arg(required = false, short, long)]
        keep: bool,

        /// Ask for confirmation before update
        #[arg(required = false, long, short)]
        ask: bool,
    },

    /// Show info about installed packages
    #[clap(name = "info", visible_alias = "list-installed")]
    ListInstalledPackages {
        /// Repository to get installed packages for
        #[arg(required = false, long, short)]
        repo_name: Option<String>,

        /// Only show the unique package install count
        #[arg(required = false, long)]
        count: bool,
    },

    /// List all available packages
    #[clap(name = "list", visible_alias = "ls")]
    ListPackages {
        /// Which repository to get the packages from
        #[arg(required = false)]
        repo_name: Option<String>,
    },

    /// Inspect package build log
    #[command(arg_required_else_help = true)]
    #[clap(name = "log")]
    Log {
        /// Package to view log for
        #[arg(required = true)]
        package: String,
    },

    /// Inspect package build script
    #[command(arg_required_else_help = true)]
    #[clap(name = "inspect")]
    Inspect {
        /// Package to view build script for
        #[arg(required = true)]
        package: String,
    },

    /// Run packages without installing to PATH
    #[command(arg_required_else_help = true)]
    #[clap(name = "run", visible_alias = "exec", visible_alias = "execute")]
    Run {
        /// Skip all prompts and use first
        #[arg(required = false, short, long)]
        yes: bool,

        /// Command to execute
        #[arg(required = true)]
        command: Vec<String>,

        /// Package id
        #[arg(required = false, long)]
        pkg_id: Option<String>,

        /// Repo name
        #[arg(required = false, short, long)]
        repo_name: Option<String>,
    },

    /// Use package from different family
    #[command(arg_required_else_help = true)]
    #[clap(name = "use")]
    Use {
        /// The package name to use alternative package for
        #[arg(required = true)]
        package_name: String,
    },

    /// Download arbitrary files
    #[command(arg_required_else_help = true)]
    #[clap(name = "download", visible_alias = "dl")]
    Download {
        /// Links to files
        #[arg(required = false)]
        links: Vec<String>,

        /// Skip all prompts and use first
        #[arg(required = false, short, long)]
        yes: bool,

        /// Output file path
        #[arg(required = false, short, long, value_hint = ValueHint::AnyPath)]
        output: Option<String>,

        /// Regex to select the asset. Only works for github downloads
        #[arg(required = false, short = 'r', long = "regex")]
        regexes: Option<Vec<String>>,

        /// Glob to select the asset.
        #[arg(required = false, short = 'g', long = "glob")]
        globs: Option<Vec<String>>,

        /// Check if the asset contains given string
        #[arg(required = false, short, long = "match")]
        match_keywords: Option<Vec<String>>,

        /// Check if the asset contains given string
        #[arg(required = false, short, long = "exclude")]
        exclude_keywords: Option<Vec<String>>,

        /// Github project
        #[arg(required = false, long)]
        github: Vec<String>,

        /// Gitlab project
        #[arg(required = false, long)]
        gitlab: Vec<String>,

        /// OCI reference
        #[arg(required = false, long)]
        ghcr: Vec<String>,

        /// Whether to use exact case matching for keywords
        #[arg(required = false, long)]
        exact_case: bool,

        /// Extract supported archive automatically
        #[arg(required = false, long)]
        extract: bool,

        /// Directory where to extract the archive
        #[arg(required = false, long)]
        extract_dir: Option<String>,

        /// Skip existing download with same file
        #[arg(required = false, long)]
        skip_existing: bool,

        /// Overwrite existing download with same file
        #[arg(required = false, long)]
        force_overwrite: bool,
    },

    /// Health check
    #[clap(name = "health")]
    Health,

    /// Generate default config
    #[clap(name = "defconfig")]
    DefConfig {
        /// Enable external repositories
        #[arg(required = false, short, long)]
        external: bool,

        /// Enable only selected repositories
        #[arg(
            required = false,
            short,
            long,
            num_args = 0..,
            value_delimiter = ',',
            value_parser = parse_default_repos_arg
        )]
        repositories: Vec<String>,
    },

    /// View env
    #[clap(name = "env")]
    Env,

    /// Garbage collection
    #[clap(name = "clean")]
    Clean {
        /// Clean cache
        #[arg(required = false, long)]
        cache: bool,

        /// Clean broken symlinks
        #[arg(required = false, long)]
        broken_symlinks: bool,

        /// Clean broken packages
        #[arg(required = false, long)]
        broken: bool,
    },

    /// Modify the soar installation
    #[command(arg_required_else_help = true)]
    #[clap(name = "self")]
    SelfCmd {
        #[clap(subcommand)]
        action: SelfAction,
    },
}
