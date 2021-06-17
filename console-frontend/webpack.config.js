const path = require('path');
const WasmPackPlugin = require('@wasm-tool/wasm-pack-plugin');
const CopyWebpackPlugin = require('copy-webpack-plugin');
const BG_IMAGES_DIRNAME = 'bgimages';

const distPath = path.resolve(__dirname, "dist");
module.exports = (env, argv) => {
    return {
        devServer: {
            contentBase: [
                distPath,
                path.resolve(__dirname, "dev")
            ],
            headers: {
                "Access-Control-Allow-Origin": "*",
                "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, PATCH, OPTIONS",
                "Access-Control-Allow-Headers": "X-Requested-With, content-type, Authorization"
            },
            historyApiFallback: {
                // should be aligned with nginx.conf
                rewrites: [
                    // don't translate endpoints
                    {
                        from: /endpoints\/.*/, to: function (context) {
                            return context.match[0];
                        }
                    },
                    // don't translate *.svg
                    {
                        from: /^(.*)\.(svg)$/, to: function (context) {
                            return context.match[0];
                        }
                    },
                    // translate fonts differently
                    {
                        from: /^(.*)(\/fonts\/.*?\.(ttf|woff.?))$/, to: function (context) {
                            return context.match[2];
                        }
                    },
                    // translate everything that is in a sub-directory (e.g. components/form) and contains a dot
                    // (e.g. components/form/main.js) to the root (e.g. main.js).
                    {
                        from: /\/.*\/(.*\..*)$/, to: function (context) {
                            return '/' + context.match[1];
                        }
                    }
                ],
                verbose: true,
            },
            compress: argv.mode === 'production',
            port: 8010,
        },
        entry: {
            main: './main.js',
            api: './api.js',
        },
        output: {
            path: distPath,
            filename: "[name].js",
            webassemblyModuleFilename: "main.wasm"
        },
        module: {
            rules: [
                {
                    test: /\.s[ac]ss$/i,
                    use: [
                        'style-loader',
                        'css-loader',
                        'sass-loader',
                    ],
                },
                {
                    test: /\.(svg|ttf|eot|woff|woff2)$/,
                    // only process modules with this loader
                    // if they live under a 'fonts' or 'pficon' directory
                    include: [
                        path.resolve(__dirname, 'node_modules/patternfly/dist/fonts'),
                        path.resolve(__dirname, 'node_modules/@patternfly/patternfly/assets/fonts'),
                        path.resolve(__dirname, 'node_modules/@patternfly/patternfly/assets/pficon'),
                    ],
                    use: {
                        loader: 'file-loader',
                        options: {
                            // Limit at 50k. larger files emited into separate files
                            limit: 5000,
                            outputPath: 'fonts',
                            name: '[name].[ext]',
                        }
                    }
                },
                {
                    test: /\.svg$/,
                    include: input => input.indexOf('background-filter.svg') > 1,
                    use: [
                        {
                            loader: 'url-loader',
                            options: {
                                limit: 5000,
                                outputPath: 'svgs',
                                name: '[name].[ext]',
                            }
                        }
                    ]
                },
                {
                    test: /\.svg$/,
                    // only process SVG modules with this loader if they live under a 'bgimages' directory
                    // this is primarily useful when applying a CSS background using an SVG
                    include: input => input.indexOf(BG_IMAGES_DIRNAME) > -1,
                    use: {
                        loader: 'svg-url-loader',
                        options: {}
                    }
                },
                {
                    test: /\.svg$/,
                    // only process SVG modules with this loader when they don't live under a 'bgimages',
                    // 'fonts', or 'pficon' directory, those are handled with other loaders
                    include: input => (
                        (input.indexOf(BG_IMAGES_DIRNAME) === -1) &&
                        (input.indexOf('fonts') === -1) &&
                        (input.indexOf('background-filter') === -1) &&
                        (input.indexOf('pficon') === -1)
                    ),
                    use: {
                        loader: 'raw-loader',
                        options: {}
                    }
                },
                {
                    test: /\.(jpg|jpeg|png|gif)$/i,
                    include: [
                        path.resolve(__dirname, 'src'),
                        path.resolve(__dirname, 'node_modules/patternfly'),
                        path.resolve(__dirname, 'node_modules/@patternfly/patternfly/assets/images'),
                        path.resolve(__dirname, 'node_modules/@patternfly/react-styles/css/assets/images'),
                        path.resolve(__dirname, 'node_modules/@patternfly/react-core/dist/styles/assets/images'),
                        path.resolve(__dirname, 'node_modules/@patternfly/react-core/node_modules/@patternfly/react-styles/css/assets/images'),
                        path.resolve(__dirname, 'node_modules/@patternfly/react-table/node_modules/@patternfly/react-styles/css/assets/images'),
                        path.resolve(__dirname, 'node_modules/@patternfly/react-inline-edit-extension/node_modules/@patternfly/react-styles/css/assets/images')
                    ],
                    use: [
                        {
                            loader: 'url-loader',
                            options: {
                                limit: 5000,
                                outputPath: 'images',
                                name: '[name].[ext]',
                            }
                        }
                    ]
                },
                {
                    test: /\.yaml$/,
                    use: [
                        {loader: 'json-loader'},
                        {loader: 'yaml-loader'}
                    ]
                },
                {
                    test: /\.css$/,
                    use: [
                        {loader: 'style-loader'},
                        {loader: 'css-loader'},
                    ]
                }
            ],
        },
        performance: {
            hints: false
        },
        experiments: {
            syncWebAssembly: true
        },
        plugins: [
            new CopyWebpackPlugin(
                {
                    patterns:
                        [
                            {from: './static', to: distPath},
                            // copy over images
                            {
                                from: path.resolve(__dirname, 'node_modules/@patternfly/patternfly/assets/images'),
                                to: path.resolve(distPath, "images")
                            },
                            {
                                // Copy the Swagger OAuth2 redirect file to the project root;
                                // that file handles the OAuth2 redirect after authenticating the end-user.
                                from: 'node_modules/swagger-ui/dist/oauth2-redirect.html',
                                to: './'
                            }
                        ]
                }),
            new WasmPackPlugin({
                crateDirectory: ".",
                extraArgs: "--no-typescript --mode=no-install",
            })
        ],
        watch: argv.mode !== 'production'
    };
};