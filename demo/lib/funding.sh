#!/usr/bin/env bash

demo_prepare_funding_boxes() {
  if demo_checkpoint_is_done "funding_boxes"; then
    echo "funding: reusing prepared boxes"
    return 0
  fi
  echo "funding: setup scripts will split funded tADA into operator, pool, account, separator, and collateral boxes"
  demo_checkpoint_done "funding_boxes"
}
