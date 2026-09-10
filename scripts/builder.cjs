const fs = require('fs');
const path = require('path');
module.exports = {
  write(filePath, content) {
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
    fs.writeFileSync(filePath, content, 'utf8');
  }
};
