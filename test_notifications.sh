#!/usr/bin/env bash

# Test script to send notifications via D-Bus

echo "Testing notification system..."

# Send a simple notification
notify-send "Test Notification" "This is a test message from cadenza-shell"

# Send a notification with different urgency levels
notify-send -u low "Low Priority" "This is a low priority notification"
notify-send -u normal "Normal Priority" "This is a normal priority notification" 
notify-send -u critical "Critical Priority" "This is a critical priority notification"

# Send a notification with an icon
notify-send -i "dialog-information" "Info Notification" "This notification has an icon"

# Send a persistent notification (no timeout)
notify-send -t 0 "Persistent" "This notification should persist until dismissed"

# Send a notification with one actions
notify-send -A button_one="Button one" "One action" "This notification has one default action" &

# Send a notification with two actions
notify-send -A button_one="Button one" -A button_two="Button two" "Two actions" "This notification has two actions" &

echo "Test notifications sent!"
