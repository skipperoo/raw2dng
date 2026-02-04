#!/bin/bash

increase_release_candidate() {
  RC=$1
  echo $RC | /usr/bin/python3 -c "import sys; rc_raw = sys.stdin.read(); print(rc_raw.split('-')[0]+'-rc'+str(int(rc_raw.split('-')[1].replace('rc', ''))+1))"
}

reset_release_candidate() {
  RC=$1
  echo $RC | /usr/bin/python3 -c "import sys; rc_raw = sys.stdin.read(); print(rc_raw.split('-')[0]+'-rc0')"
}

remove_release_candidate() {
  RC=$1
  echo $RC | /usr/bin/python3 -c "import sys; rc_raw = sys.stdin.read(); print(rc_raw.split('-')[0])"
}

if [[ "$#" == "2" ]]; then
  TARGET=$1
else
  TARGET="patch"
fi

#get highest tag number
VERSION=$(git describe --abbrev=0 --tags)

VERSION_BITS=(${VERSION//./ })
VNUM1=${VERSION_BITS[0]}
VNUM2=${VERSION_BITS[1]}
VNUM3=${VERSION_BITS[2]}

VNUM1=$(echo $VNUM1 | sed 's/v//')

if [[ "$1" == "major" ]]; then
  echo "Update major version"
  VNUM1=$((VNUM1 + 1))
  VNUM2=0
  VNUM3=0
elif [[ "$1" == "minor" ]]; then
  echo "Update minor version"
  VNUM2=$((VNUM2 + 1))
  VNUM3=0
else
  # if [[ "$VNUM3" == *"rc"* ]]; then
  #     echo "Updating release candidate"
  #     VNUM3=$(increase_release_candidate $VNUM3)
  # else
  echo "Updating patch version"
  VNUM3=$((VNUM3 + 1))
  # fi
fi

#create new tag
NEW_TAG="v$VNUM1.$VNUM2.$VNUM3"

echo "Updating $VERSION to $NEW_TAG"

#get current hash and see if it already has a tag
GIT_COMMIT=$(git rev-parse HEAD)
NEEDS_TAG=$(git describe --contains $GIT_COMMIT 2>/dev/null)

#only tag if no tag already (would be better if the git describe command above could have a silent option)
if [ -z "$NEEDS_TAG" ]; then
  echo "Tagged with $NEW_TAG (Ignoring fatal:cannot describe - this means commit is untagged) "
  git tag $NEW_TAG
  git push --tags
else
  echo "Already a tag on this commit"
fi

sed -i "s@VERSION=.*@VERSION=$(git describe --tags --abbrev=0)@g" ./zagent-manager.sh
