# syntax=docker/dockerfile:1
   
FROM archlinux:base-devel
WORKDIR /

RUN pacman --noconfirm -Syy && \
    pacman --noconfirm -Syu && \
    pacman --noconfirm -S sudo && \
    useradd -m build && \
    echo "build ALL=(ALL) NOPASSWD :ALL" > /etc/sudoers.d/10-build

USER build
WORKDIR /home/build
COPY dependencies dependencies

RUN sudo pacman --noconfirm --needed -S git && \
    git clone https://aur.archlinux.org/yay-bin.git && \
    cd yay-bin && makepkg --noconfirm -si

RUN sudo pacman --noconfirm -S --needed --overwrite "*" $(cat dependencies/arch_run_default.txt | tr '\n' ' ') && \
    yay --noconfirm -S --needed --overwrite "*" $(cat dependencies/arch_run_default_aur.txt | tr '\n' ' ') && \
    sudo pacman --noconfirm -S --needed --overwrite "*" python-pip

