# AionR for Aarch64/Raspberry Pi 4B

## Install the Kernel

Follow this guide to install the Aion Rust kernel on your PI 4B.

### System Requirements

- Ubuntu 18.04 64bit
- 4GB RAM
- Micro SD card 64/128 GB (Current Mainnet DB about 30GB)

### Prerequisites Installation
1. There are two options to install ubuntu 18.04 64bit on PI4

   1. Ubuntu 18.04 for PI 4 is not offical released, you can use the [unofficail preintsalled image](https://github.com/TheRemote/Ubuntu-Server-raspi4-unofficial/releases) instead. 
   2. Install officail 19.10 release from [ubuntu wiki](https://wiki.ubuntu.com/ARM/RaspberryPi), then install Docker and build a ubuntu arm64v8/ubuntu:18.04 docker image.
   
2. Update your system and install the build dependencies:

    ```bash
    sudo apt-get update
    sudo apt install g++ gcc libjsoncpp-dev python-dev libudev-dev llvm-4.0-dev cmake wget curl git pkg-config lsb-release -y
    ```

3. Install Rust `v1.28.0`:

    ```bash
    curl https://sh.rustup.rs -sSf | sh -s -- --default-toolchain 1.28.0
    ```

    Select option `1` when prompted.

4. Initialize the Rust install and check that it is working:

    ```bash
    source $HOME/.cargo/env
    cargo --version

    > cargo 1.28.0 (96a2c7d16 2018-07-13)
    ```

5. Install Boost `v1.65.1`
    
    ```bash
    sudo apt-get install libboost-all-dev -y
    ```

6. Install JAVA JDK

    ```bash
    sudo apt-get install openjdk-11-jdk -y
    ```

7. Install Apache Ant 10
    * [Apache Ant 10](http://mirror.reverse.net/pub/apache//ant/binaries/apache-ant-1.10.7-bin.tar.gz)

8. Set Environment Variables
    ```bash
    export JAVA_HOME=<jdk_directory_location>
    export ANT_HOME=<apache_ant_directory>	
    export LIBRARY_PATH=$JAVA_HOME/lib/server
    export PATH=$PATH:$JAVA_HOME/bin:$ANT_HOME/bin
    export LD_LIBRARY_PATH=$LIBRARY_PATH:/usr/local/lib
    ```
### Build the Kernel

Once you have installed the prerequisites, follow these steps to build the kernel.

1. Download the Aion Rust git repository:

    ```bash
    git clone https://github.com/vito11/aionr.git
    cd aionr
    ```

2. Build the kernel from source:

    ```bash
    ./resources/package.sh aionr-package
    ```

    `aionr-package` is the name that will be given to the Rust package when it as finished building. You can set this to anything you want by changing the last argument in the script call:

    ```bash
    ./resources/package.sh [example-package-name]
    ```

    The package takes about 10 minutes to finish building.

3. When the build has finished, you can find the finished binary at `package/aionr-package`.

## Launch Aion Rust Kernel

1. Navigate to the binary location:

    ```bash
    cd package/aionr-package
    ```

2. Run the `aion` package. Make sure to include any commands you want the kernel to execute. You can find more information on supplying commands in the [user manual](https://github.com/aionnetwork/aionr/wiki/User-Manual#launch-rust-kernel).
Kernel will print **configuration path**, **genesis file path**, **db directory** and **keystore location** at the top of its log.

**We provides quick launch scripts to connect to Mainnet, Mastery and custom network. Running the quick scripts will load the configuration and the genesis in each network folder. You can modify those files in each directory. See launch examples [Kernel Deployment Examples](https://github.com/aionnetwork/aionr/wiki/Kernel-Deployment-Examples)**

```bash
$ ./mainnet.sh

>   ____                 _   _ 
>  / __ \       /\      | \ | |
> | |  | |     /  \     |  \| |
> | |  | |    / /\ \    | . ` |
> | |__| |   / ____ \   | |\  |
>  \____/   /_/    \_\  |_| \_|
>
>
> 2019-11-06 13:54:03        build: Aion(R)/v1.0.0.706f7dc/x86_64-linux-gnu/rustc-1.28.0
> 2019-11-06 13:54:03  config path: kernel_package_path/mainnet/mainnet.toml
> 2019-11-06 13:54:03 genesis path: kernel_package_path/mainnet/mainnet.json
> 2019-11-06 13:54:03    keys path: /home/username/.aion/keys/mainnet
> 2019-11-06 13:54:03      db path: /home/username/.aion/chains/mainnet/db/a98e36807c1b0211
> 2019-11-06 13:54:03      binding: 0.0.0.0:30303
> 2019-11-06 13:54:03      network: Mainnet
> 2019-11-06 13:54:10      genesis: 30793b4ea012c6d3a58c85c5b049962669369807a98e36807c1b02116417f823

```

