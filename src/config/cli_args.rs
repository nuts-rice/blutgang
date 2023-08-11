use clap::{
    Arg,
    Command,
};

pub fn create_match() -> clap::Command {
    let matches = Command::new("blutgang")
        .version("0.1.0")
        .author("makemake <vukasin@gostovic.me>")
        .about("Tool for replaying historical transactions. Designed to be used with anvil or hardhat.")
        .arg(Arg::new("rpc_list")
            .long("rpc_list")
            .short('r')
            .num_args(1..)
            .default_value("")
            .required(true)
            .help("CSV list of rpcs"))
        .arg(Arg::new("port")
            .long("port")
            .short('p')
            .num_args(1..)
            .default_value("3000")
            .help("port to listen to"))
        .arg(Arg::new("db")
            .long("db")
            .short('d')
            .num_args(1..)
            .default_value("blutgang-cache")
            .help("Database path"))
        .arg(Arg::new("clear")
            .long("clear")
            .num_args(0..)
            .help("Clear cache"));

    return matches;
}