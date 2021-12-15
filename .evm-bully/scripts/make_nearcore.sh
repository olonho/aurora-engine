#!/bin/bash -e

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
source $SCRIPT_DIR/common.sh


if [[ ! -f $NEARCORE_BINARY_PATH ]] && [[ ! -z $USE_CACHE_UTIL ]]; then
    echo "nearcore: trying to restore from cache..."
    cache-util restore $NEARCORE_CACHE_KEY:$NEARCORE_BINARY_PATH || true
    if [[ -f $NEARCORE_BINARY_PATH ]]; then
        exit 0
    fi
fi

# Unfortunately, we can't use original nearcore daemon,
# because we need to change it's hardcoded realtime-config before compilation
# (to avoid gas-exceeded issues)
#
# if [[ ! -f $NEARCORE_BINARY_PATH ]] && [[ $DOWNLOAD_NEARCORE = true ]]; then
#     echo "nearcore: trying to download..."
#     curl -L \
#         https://s3.us-west-1.amazonaws.com/build.nearprotocol.com/nearcore/$(uname)/master/${NEARCORE_VERSION}/neard \
#         -o $NEARCORE_BINARY_PATH \
#         || true
#     chmod +x $NEARCORE_BINARY_PATH || true
# fi

if [[ ! -f $NEARCORE_BINARY_PATH ]]; then
    echo "nearcore: checkouting repo..."
    checkout_repo $NEARCORE_REPO_PATH https://github.com/near/nearcore.git $NEARCORE_VERSION

    echo "nearcore: patching realtime config..."
    for realtime_config in ${NEARCORE_REPO_PATH}/core/primitives/res/runtime_configs/*.json; do
        sed -i '/"max_gas_burnt"/c\      "max_gas_burnt": 2000000000000000,' $realtime_config
        sed -i '/"max_gas_burnt_view"/c\      "max_gas_burnt_view": 2000000000000000,' $realtime_config
        sed -i '/"max_total_prepaid_gas"/c\      "max_total_prepaid_gas": 3000000000000000,' $realtime_config
    done

    echo "nearcore: building..."
    cd $NEARCORE_REPO_PATH
    cargo build --package neard --features nightly_protocol_features --release
    cp target/release/neard $NEARCORE_BINARY_PATH
fi

if [[ -f $NEARCORE_BINARY_PATH ]] && [[ ! -z $USE_CACHE_UTIL ]]; then
    echo "nearcore: saving to cache..."
    cache-util save $NEARCORE_CACHE_KEY:$NEARCORE_BINARY_PATH || true
fi
