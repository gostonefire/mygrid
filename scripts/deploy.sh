#!/bin/bash

REPO_NAME=$1
DEV_DIR=$2
SUB_SCRIPT_LOG=$3

HOME="/home/petste"
APP_DIR="MyGrid"
SERVICE_NAME="mygrid"

if [ -z "$REPO_NAME" ] || [ -z "$DEV_DIR" ] || [ -z "$SUB_SCRIPT_LOG" ]; then
  echo "Usage: $0 <repo_name> <dev_dir> <sub_script_log>"
  exit 1
fi

# Load common.sh (it is within /usr/local/bin) for common functions
source common.sh

# Get the owner of the DEV_DIR
DEV_USER=$(stat -c '%U' "$DEV_DIR")

# Function containing the logic to be run as the directory owner
run_as_user() {
  # Load dependencies
  source common.sh

  run_cmd "mkdir -p $HOME/$APP_DIR/config"       "$SUB_SCRIPT_LOG" "could not create $HOME/$APP_DIR/config..."
  run_cmd "mkdir -p $HOME/$APP_DIR/logs"         "$SUB_SCRIPT_LOG" "could not create $HOME/$APP_DIR/logs..."
  run_cmd "mkdir -p $HOME/$APP_DIR/last_version" "$SUB_SCRIPT_LOG" "could not create $HOME/$APP_DIR/last_version..."
  run_cmd "cd $DEV_DIR/$REPO_NAME"               "$SUB_SCRIPT_LOG" "could not change directory to $DEV_DIR/$REPO_NAME..."

  if [ -f "$HOME/$APP_DIR/$REPO_NAME" ]; then
    run_cmd "mv $HOME/$APP_DIR/$REPO_NAME $HOME/$APP_DIR/last_version/" "$SUB_SCRIPT_LOG" "could not move $HOME/$APP_DIR/$REPO_NAME to $HOME/$APP_DIR/last_version/"
  fi

  run_cmd "cp ./target/release/$REPO_NAME $HOME/$APP_DIR/" "$SUB_SCRIPT_LOG" "could not copy ./target/release/$REPO_NAME to $HOME/$APP_DIR/..."

  ### Add any extra deploy features to be run as dev user from here ###
  run_cmd "cp ./config/config.toml $HOME/$APP_DIR/config/"     "$SUB_SCRIPT_LOG" "could not copy ./config/config.toml to $HOME/$APP_DIR/config/..."
  run_cmd "cp ./systemd/$SERVICE_NAME.service $HOME/$APP_DIR/" "$SUB_SCRIPT_LOG" "could not copy ./systemd/$SERVICE_NAME.service to $HOME/$APP_DIR/..."
  run_cmd "cp ./systemd/start.sh $HOME/$APP_DIR/"              "$SUB_SCRIPT_LOG" "could not copy ./systemd/start.sh to $HOME/$APP_DIR/..."
  run_cmd "chmod 755 $HOME/$APP_DIR/start.sh"                  "$SUB_SCRIPT_LOG" "could not make start script executable"

  ### until to here! ##################################################
}

########## From here the script will be run as root, so add any commands such as systemctl etc. from here ##########
if systemctl is-active --quiet "$SERVICE_NAME.service"; then
  systemctl stop --quiet "$SERVICE_NAME.service"
fi
########## until to here! ##########################################################################################

# Export variables so the subshell can see them, then run the function as the owner
export REPO_NAME DEV_DIR HOME APP_DIR SUB_SCRIPT_LOG SERVICE_NAME

sudo -u "$DEV_USER" -E bash -c "$(declare -f run_as_user); run_as_user"
EXIT_CODE=$?
if [ $EXIT_CODE -ne 0 ]; then
  exit $EXIT_CODE
fi

########## From here the script will be run as root, so add any commands such as systemctl etc. from here ##########
run_cmd "cp $HOME/$APP_DIR/$SERVICE_NAME.service /lib/systemd/system/" "$SUB_SCRIPT_LOG" "failed to copy $SERVICE_NAME.service to /lib/systemd/system/..."
run_cmd "systemctl daemon-reload"                                      "$SUB_SCRIPT_LOG" "failed to reload systemd daemon"
run_cmd "systemctl enable --now $SERVICE_NAME.service"                 "$SUB_SCRIPT_LOG" "could not enable and start $SERVICE_NAME.service..."

########## until to here! ##########################################################################################
