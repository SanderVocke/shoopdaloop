#!/usr/bin/env bash
set -Eeuo pipefail

input_path=${1:?usage: publish_dev_web_release.sh <artifact-file-or-directory>}

for name in ASSET_NAME CANDIDATE_SHA GITHUB_REPOSITORY RELEASE_TAG SOURCE_RUN_ID SOURCE_RUN_URL; do
    if [[ -z ${!name:-} ]]; then
        echo "Required environment variable $name is not set" >&2
        exit 1
    fi
done

if [[ ! $CANDIDATE_SHA =~ ^[0-9a-f]{40,64}$ ]]; then
    echo "Invalid candidate commit: $CANDIDATE_SHA" >&2
    exit 1
fi
if [[ ! $SOURCE_RUN_ID =~ ^[0-9]+$ || ! $ASSET_NAME =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "Invalid run ID or asset name" >&2
    exit 1
fi
if [[ -n ${LEGACY_ASSET_NAME:-} && ! $LEGACY_ASSET_NAME =~ ^[A-Za-z0-9._-]+$ ]]; then
    echo "Invalid legacy asset name" >&2
    exit 1
fi

if [[ -d $input_path ]]; then
    mapfile -d '' candidates < <(find "$input_path" -type f -name '*.html' -print0)
    if (( ${#candidates[@]} != 1 )); then
        echo "Expected exactly one HTML file in $input_path, found ${#candidates[@]}" >&2
        exit 1
    fi
    source_file=${candidates[0]}
elif [[ -f $input_path && $input_path == *.html ]]; then
    source_file=$input_path
else
    echo "Artifact input is not an HTML file: $input_path" >&2
    exit 1
fi

if [[ ! -s $source_file ]]; then
    echo "Artifact is empty: $source_file" >&2
    exit 1
fi

repo_api="repos/$GITHUB_REPOSITORY"
master_sha() {
    gh api "$repo_api/git/ref/heads/master" --jq .object.sha
}

if [[ $(master_sha) != "$CANDIDATE_SHA" ]]; then
    echo "Skipping $CANDIDATE_SHA because it is no longer the head of master"
    exit 0
fi

release_id=$(gh api "$repo_api/releases/tags/$RELEASE_TAG" --jq .id)
if [[ $(gh api "$repo_api/releases/tags/$RELEASE_TAG" --jq .prerelease) != true ]]; then
    echo "Release $RELEASE_TAG exists but is not a prerelease" >&2
    exit 1
fi

asset_id() {
    local asset_name=$1
    gh api --paginate "$repo_api/releases/$release_id/assets?per_page=100" \
        --jq ".[] | select(.name == \"$asset_name\") | .id" |
        tail -n 1
}

delete_asset() {
    gh api --method DELETE "$repo_api/releases/assets/$1" >/dev/null
}

rename_asset() {
    gh api --method PATCH "$repo_api/releases/assets/$1" -f "name=$2" >/dev/null
}

candidate_name="$ASSET_NAME.candidate-$SOURCE_RUN_ID"
backup_name="$ASSET_NAME.previous"
staging_dir=$(mktemp -d)
staged_file="$staging_dir/$candidate_name"
notes_file="$staging_dir/release-notes.md"
verify_dir="$staging_dir/verify"
cp -- "$source_file" "$staged_file"
local_size=$(stat --format=%s "$staged_file")
local_sha256=$(sha256sum "$staged_file" | cut -d ' ' -f 1)

old_moved=false
new_promoted=false
committed=false

rollback() {
    local status=$1
    trap - ERR INT TERM
    set +e

    if [[ $committed != true ]]; then
        local fixed_id backup_id_value
        fixed_id=$(asset_id "$ASSET_NAME")
        backup_id_value=$(asset_id "$backup_name")

        if [[ $old_moved == true ]]; then
            if [[ -n $fixed_id ]]; then
                delete_asset "$fixed_id"
            fi
            if [[ -n $backup_id_value ]]; then
                rename_asset "$backup_id_value" "$ASSET_NAME"
            fi
        elif [[ $new_promoted == true && -n $fixed_id ]]; then
            delete_asset "$fixed_id"
        fi
    fi

    local candidate_id
    candidate_id=$(asset_id "$candidate_name")
    if [[ -n $candidate_id ]]; then
        delete_asset "$candidate_id"
    fi
    rm -rf "$staging_dir"
    exit "$status"
}

trap 'rollback $?' ERR
trap 'rollback 130' INT
trap 'rollback 143' TERM

stale_candidate_id=$(asset_id "$candidate_name")
if [[ -n $stale_candidate_id ]]; then
    delete_asset "$stale_candidate_id"
fi

gh release upload "$RELEASE_TAG" "$staged_file" --repo "$GITHUB_REPOSITORY"
uploaded_id=$(asset_id "$candidate_name")
if [[ -z $uploaded_id ]]; then
    echo "Uploaded candidate asset was not found" >&2
    rollback 1
fi

remote_size=$(gh api "$repo_api/releases/assets/$uploaded_id" --jq .size)
if [[ $remote_size != "$local_size" ]]; then
    echo "Uploaded candidate size differs from the local artifact" >&2
    rollback 1
fi

mkdir "$verify_dir"
gh release download "$RELEASE_TAG" \
    --repo "$GITHUB_REPOSITORY" \
    --pattern "$candidate_name" \
    --dir "$verify_dir"
if ! cmp --silent "$staged_file" "$verify_dir/$candidate_name"; then
    echo "Downloaded candidate differs from the local artifact" >&2
    rollback 1
fi

if [[ $(master_sha) != "$CANDIDATE_SHA" ]]; then
    echo "Skipping $CANDIDATE_SHA because master advanced while it was staged"
    candidate_id=$(asset_id "$candidate_name")
    if [[ -n $candidate_id ]]; then
        delete_asset "$candidate_id"
    fi
    trap - ERR INT TERM
    rm -rf "$staging_dir"
    exit 0
fi

fixed_id=$(asset_id "$ASSET_NAME")
backup_id=$(asset_id "$backup_name")
if [[ -z $fixed_id && -n $backup_id ]]; then
    rename_asset "$backup_id" "$ASSET_NAME"
    fixed_id=$backup_id
    backup_id=
elif [[ -n $fixed_id && -n $backup_id ]]; then
    delete_asset "$backup_id"
    backup_id=
fi

if [[ $(master_sha) != "$CANDIDATE_SHA" ]]; then
    echo "Skipping $CANDIDATE_SHA because master advanced before promotion"
    candidate_id=$(asset_id "$candidate_name")
    if [[ -n $candidate_id ]]; then
        delete_asset "$candidate_id"
    fi
    trap - ERR INT TERM
    rm -rf "$staging_dir"
    exit 0
fi

if [[ -n $fixed_id ]]; then
    rename_asset "$fixed_id" "$backup_name"
    old_moved=true
fi
rename_asset "$uploaded_id" "$ASSET_NAME"
new_promoted=true

published_at=$(date --utc +'%Y-%m-%dT%H:%M:%SZ')
short_sha=${CANDIDATE_SHA:0:12}
cat >"$notes_file" <<EOF
Rolling standalone web development preview from the latest \`master\` commit that produced the optimized HTML artifact. This is not an official release. The \`$RELEASE_TAG\` tag is a stable preview locator; the commit below identifies the attached build.

- Commit: [$short_sha](https://github.com/$GITHUB_REPOSITORY/commit/$CANDIDATE_SHA)
- Workflow run: [$SOURCE_RUN_ID]($SOURCE_RUN_URL)
- SHA-256: \`$local_sha256\`
- Size: $local_size bytes
- Published: $published_at

<!-- development-web-commit:$CANDIDATE_SHA -->
EOF

gh release edit "$RELEASE_TAG" \
    --repo "$GITHUB_REPOSITORY" \
    --title "Latest development web build" \
    --prerelease \
    --notes-file "$notes_file"
committed=true
trap - ERR INT TERM

backup_id=$(asset_id "$backup_name")
if [[ -n $backup_id ]] && ! delete_asset "$backup_id"; then
    echo "Warning: could not remove backup asset $backup_name" >&2
fi
if [[ -n ${LEGACY_ASSET_NAME:-} ]]; then
    legacy_id=$(asset_id "$LEGACY_ASSET_NAME")
    if [[ -n $legacy_id ]] && ! delete_asset "$legacy_id"; then
        echo "Warning: could not remove legacy asset $LEGACY_ASSET_NAME" >&2
    fi
fi
rm -rf "$staging_dir"

echo "Published $CANDIDATE_SHA to https://github.com/$GITHUB_REPOSITORY/releases/download/$RELEASE_TAG/$ASSET_NAME"
