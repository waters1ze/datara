#!/usr/bin/env node
const { runBinary } = require('../lib/binary');
runBinary('forgen', process.argv.slice(2));
