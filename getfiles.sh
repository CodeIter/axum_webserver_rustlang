#!/usr/bin/env bash
# getfiles.sh
# This script lists and concatenates the contents of selected files
# in the current directory and its subdirectories, excluding certain
# directories.
# Useful for feeding the contents of files to a language model for analysis
# or summarization.

echo "Files: ${@}"

{
    tree --gitignore -I 'node_modules|.git|vendor|target'

    for file in "${@}" ; do
        echo "=================================================================="
        echo File: "$file"
        echo "=================================================================="
        cat -n "$file"
        echo "=================================================================="
    done

} | tee "getfiles.txt"
