#!/bin/bash

MINIDUMP_FILE=$1
VERSION=$2
EMAIL=$3
COMMENTS=$4
URL="https://shoopdaloop.bugsplat.com/post/bp/crash/postBP.php"

curl \
    -F prod=shoopdaloop \
    -F ver=$VERSION \
    -F key=dev \
    -F email=$EMAIL \
    -F comments=$COMMENTS \
    -F upload_file_minidump=@$MINIDUMP_FILE \
    $URL