# Installs the Node release pinned here and pi from this folder's lockfile into a task image, with install scripts
# off, so every trial of every task runs the same agent and installs nothing over the trial's closed network.
set -eu
node=v22.23.4
curl -fsSLo /tmp/node.tar.gz "https://nodejs.org/dist/$node/node-$node-linux-x64.tar.gz"
echo "b8492284c76a54baf58d61668dd0e41ee0fc98adabf9503d30f05d08f55b4406  /tmp/node.tar.gz" | sha256sum -c -
mkdir /opt/node
tar -xzf /tmp/node.tar.gz -C /opt/node --strip-components=1 --no-same-owner
rm /tmp/node.tar.gz
# npm otherwise asks the registry for its own updates whenever it runs, which a trial's network refuses.
mkdir -p /opt/node/etc
printf 'update-notifier=false\naudit=false\nfund=false\n' > /opt/node/etc/npmrc
ln -s /opt/node/bin/node /opt/node/bin/npm /usr/local/bin/
cd /opt/pi
npm ci --ignore-scripts --cache /tmp/npm-cache
rm -rf /tmp/npm-cache
ln -s /opt/pi/node_modules/.bin/pi /usr/local/bin/pi
