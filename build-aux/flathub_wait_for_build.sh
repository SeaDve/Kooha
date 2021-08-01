#!/bin/bash

# Forked from https://github.com/geigi/cozy/blob/master/.ci/flathub_wait_for_build.sh

URL_RUNNING_BUILDS="https://flathub.org/builds/api/v2/builders/32/builds?complete=false&flathub_name__eq=io.github.seadve.Kooha&order=-number&property=owners&property=workername"
URL_LAST_BUILD="https://flathub.org/builds/api/v2/builders/32/builds?flathub_name__eq=io.github.seadve.Kooha&flathub_repo_status__gt=1&limit=1&order=-number&property=owners&property=workername"

function wait_for_build_triggered {
    for i in {0..30}
    do
        sleep 1
        builds_in_progress=$(curl $URL_RUNNING_BUILDS | json meta.total)
        if (( builds_in_progress > 0 )); then
            echo "$builds_in_progress build(s) in progress."
            return 0
        fi
    done

    echo "No build in progress."
    return 1
}

wait_for_build_triggered
build_triggered=$?
if (( $build_triggered > 0 )); then
    exit 1
fi

builds_in_progress=$(curl $URL_RUNNING_BUILDS | json meta.total)
counter=0

while [[ $builds_in_progress != [0] ]]
do
    echo "$builds_in_progress build(s) in progress."
    sleep 5
    builds_in_progress=$(curl $URL_RUNNING_BUILDS | json meta.total)
    counter=$((counter+5))
    if (( counter > 1800 )); then
        echo "Build longer than 30min, failing!"
        exit 1
    fi
done

result=$(curl $URL_LAST_BUILD | json builds[0].results)
if (( builds_in_progress > 0 )); then
    echo "Build failed."
    exit 1
fi

echo "Build succeeded."
exit 0

