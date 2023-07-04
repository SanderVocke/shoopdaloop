#!/bin/bash

DESCRIPTION=$(cat $1 | grep -E '^description[ ]*=[ ]*"[^"]+"' | sed -r 's/description[ ]*=[ ]*"([^"]+)"/\1/g')
VERSION=$(cat $1 | grep -E '^version[ ]*=[ ]*"[^"]+"' | sed -r 's/version[ ]*=[ ]*"([^"]+)"/\1/g')

if [[ "$2" == "version" ]]; then
    echo $VERSION
fi
if [[ "$2" == "description" ]]; then
    echo $DESCRIPTION
fi