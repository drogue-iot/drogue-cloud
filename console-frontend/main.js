import './node_modules/@patternfly/patternfly/patternfly.scss';
import './node_modules/@patternfly/patternfly/patternfly-addons.scss';
import './static/style.scss';

import("./pkg").then(module => {
    module.run_app();
});