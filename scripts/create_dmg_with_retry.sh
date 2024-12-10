#!/bin/bash

# Retry logic for create-dmg command
max_tries=10
i=0
until create-dmg \
        --no-internet-enable \
        --format ULFO \
        --background ../packaging/macos/dmg-background.png \
        --hide-extension 'Notes Better.app' \
        --icon 'Notes Better.app' 180 170 \
        --icon-size 160 \
        --text-size 12 \
        --volname Notes \
        --volicon ../src/images/notes_icon.icns \
        --window-size 660 400 \
        --app-drop-link 480 170 \
        "${{ steps.vars.outputs.file_name }}" \
        'Notes Better.app'
do
    if [ $i -eq $max_tries ]
    then
        echo 'Error: create-dmg did not succeed even after 10 tries.'
        exit 1
    fi
    i=$((i+1))
done
