use anyhow::{bail, Result};
use clap::builder::{IntoResettable, Str, StyledStr};
use clap::{Arg, ArgMatches, ColorChoice};
use rustyline::error::ReadlineError;
use std::collections::HashMap;
use std::ffi::OsString;
use rustyline::DefaultEditor;

pub use clap;
pub use rustyline;
pub use shell_words;


type HandleFn<'ctx, Ctx> =
    dyn Fn(&Command<'ctx, Ctx>, &ArgMatches, &mut Ctx) -> Result<()> + 'ctx;

pub struct Command<'ctx, Ctx: 'ctx> {
    cmd: clap::Command,
    handler: Box<HandleFn<'ctx, Ctx>>,
    subcmds: HashMap<String, Self>,
}

impl<'ctx, Ctx: 'ctx> Command<'ctx, Ctx> {
    /// Create a new command.
    pub fn new<S: Into<Str>>(name: S) -> Self {
        Self {
            cmd: clap::Command::new(name),
            handler: Box::new(Self::dispatch_subcmd),
            subcmds: HashMap::new(),
        }
    }

    /// (Re)Sets this command's app name.
    pub fn name<S: Into<Str>>(mut self, name: S) -> Self {
        self.cmd = self.cmd.name(name);
        self
    }

    pub fn alias<S: IntoResettable<Str>>(mut self, name: S) -> Self {
        self.cmd = self.cmd.alias(name);
        self
    }

    pub fn aliases(mut self, names: impl IntoIterator<Item = impl Into<Str>>) -> Self {
        self.cmd = self.cmd.aliases(names);
        self
    }

    pub fn about<O: IntoResettable<StyledStr>>(mut self, about: O) -> Self {
        self.cmd = self.cmd.about(about);
        self
    }

    pub fn version<S: IntoResettable<Str>>(mut self, ver: S) -> Self {
        self.cmd = self.cmd.version(ver);
        self
    }

    pub fn author<S: IntoResettable<Str>>(mut self, author: S) -> Self {
        self.cmd = self.cmd.author(author);
        self
    }

    pub fn color(mut self, color: ColorChoice) -> Self {
        self.cmd = self.cmd.color(color);
        self
    }

    #[allow(dead_code)]
    pub fn display_order(mut self, ord: usize) -> Self {
        self.cmd = self.cmd.display_order(ord);
        self
    }

    pub fn subcommand_required_else_help(mut self, yes: bool) -> Self {
        self.cmd = self
            .cmd
            .subcommand_required(yes)
            .arg_required_else_help(yes);
        self
    }

    pub fn arg<A: Into<Arg>>(mut self, a: A) -> Self {
        self.cmd = self.cmd.arg(a);
        self
    }

    pub fn handler<H>(mut self, handler: H) -> Self
    where
        H: Fn(&Self, &ArgMatches, &mut Ctx) -> Result<()> + 'ctx,
    {
        self.handler = Box::new(handler);
        self
    }

    /// Add subcommand for this Command.
    pub fn subcommand(mut self, subcmd: Self) -> Self {
        let subcmd_name = subcmd.get_name().to_owned();

        self.cmd = self.cmd.subcommand(subcmd.cmd.clone());
        self.subcmds.insert(subcmd_name, subcmd);

        self
    }

    /// Same as [`subcommand`], but accept multiple subcommands.
    ///
    /// [`Command::subcommand`]: Command::subcommand
    pub fn subcommands<I>(self, subcmds: I) -> Self
    where
        I: IntoIterator<Item = Self>,
    {
        // just a fancy loop!
        subcmds
            .into_iter()
            .fold(self, |this, subcmd| this.subcommand(subcmd))
    }

    pub fn with_completions_subcmd(self) -> Self {
        let completions_without_handler = Self::new("completions")
            .about("Generate completions for current shell. Add the output script to `.profile` or `.bashrc` etc. to make it effective.")
            .arg(
                Arg::new("shell")
                    .required(true)
                    .value_parser([
                        "bash",
                        "zsh",
                        "powershell",
                        "fish",
                        "elvish",
                    ]),
            );

        let cmd_for_completions = self
            .cmd
            .clone()
            .subcommand(completions_without_handler.cmd.clone());
        let completions = completions_without_handler.handler(move |_cmd, m, _ctx| {
            let shell: clap_complete::Shell =
                m.get_one::<String>("shell").unwrap().parse().unwrap();
            let mut stdout = std::io::stdout();
            let bin_name = cmd_for_completions.get_name();
            clap_complete::generate(
                shell,
                &mut cmd_for_completions.clone(),
                bin_name,
                &mut stdout,
            );
            Ok(())
        });

        self.subcommand(completions)
    }

    #[allow(unused)]
    pub fn exec(&self, ctx: &mut Ctx) -> Result<()> {
        let m = self.cmd.clone().get_matches();
        self.exec_with(&m, ctx)
    }

    /// Execute this command with context and args.
    pub fn exec_with(&self, m: &ArgMatches, ctx: &mut Ctx) -> Result<()> {
        (self.handler)(self, m, ctx)
    }

    pub fn exec_from<I, T>(&self, iter: I, ctx: &mut Ctx) -> Result<()>
    where
        I: IntoIterator<Item = T>,
        T: Into<OsString> + Clone,
    {
        let m = self.cmd.clone().try_get_matches_from(iter)?;
        self.exec_with(&m, ctx)
    }

    pub fn dispatch_subcmd(&self, m: &ArgMatches, ctx: &mut Ctx) -> Result<()> {
        if let Some((subcmd_name, subcmd_matches)) = m.subcommand() {
            if let Some(subcmd) = self.subcmds.get(subcmd_name) {
                subcmd.exec_with(subcmd_matches, ctx)?;
            } else {
                // TODO: this may be an unreachable branch.
                bail!("no subcommand handler for `{}`", subcmd_name);
            }
        }
        Ok(())
    }

    /// Get name of the underlaying clap App.
    pub fn get_name(&self) -> &str {
        self.cmd.get_name()
    }

    /// Get matches from the underlaying clap App.
    pub fn get_matches(&self) -> ArgMatches {
        self.cmd.clone().get_matches()
    }

    /// Get matches from the given cmd.
    pub fn get_matches_from(&self, cmd: &[&str]) -> ArgMatches {
        self.cmd.clone().get_matches_from(cmd)
    }

    #[allow(unused)]
    pub fn get_all_aliases(&self) -> impl Iterator<Item = &str> + '_ {
        self.cmd.get_all_aliases()
    }
}

pub fn repl<'ctx, Ctx>(cmd: Command<'ctx, Ctx>, mut ctx: Ctx, prompt: &str) {
    let m = cmd.get_matches();
    cmd.exec_with(&m, &mut ctx).unwrap();

    if m.subcommand().is_none() {
        let mut editor = DefaultEditor::new().unwrap();
        loop {
            let line = editor.readline(prompt);
            match line {
                Ok(line) => {
                    editor.add_history_entry(&line).unwrap();

                    let args = match shell_words::split(&line) {
                        Ok(args) => args,
                        Err(e) => {
                            println!("parse error: `{}`", e);
                            continue;
                        }
                    };
                    let input = std::iter::once(cmd.get_name().into()).chain(args);
                    if let Err(e) = cmd.exec_from(input, &mut ctx) {
                        println!("{:?}", e);
                    }
                }
                Err(ReadlineError::Eof) => break,
                Err(ReadlineError::Interrupted) => println!("press CTRL-D to exit"),
                Err(e) => {
                    println!("readline error {}", e);
                    break;
                }
            }
        }
    }
}
