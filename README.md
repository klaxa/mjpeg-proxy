mjpeg-proxy
===========

After dealing with network cameras that produce arbitrary framerate mjpeg streams, I was fed up enough to start this project.
This software is a proxy for mjpeg streams, reading in arbitrary framerate streams and serving them to clients at a smooth configurable fixed framerate.

Usage
-----

Check out the source and compile it using `cargo build` or `cargo build --release`. The configuration is read from `config.yml` in the current working directory. To start the server, execute the binary in a path with a configuration file  (e.g. `target/debug/mjpeg-proxy` from the repository's root directory).

Configuration
-------------

A sample configuration file is supplied using all currently available configuration values:

```yaml
default:
  address: 127.0.0.1
  port: 8080
  streams:
    cam_1:
      path: cam-1.mjpeg
      url: http://some-stream.com/stream.mjpeg
      fps: 25
    cam_2:
      path: cam-2.mjpeg
      url: http://some-stream.com/stream2.mjpeg
      fps: 25
```


### root element

The root element of the configuration object is a dictionary of servers. In this example the server is called `default`.
### Server configuration
#### address

Configures the address to listen on. Default: `127.0.0.1`

#### port

Configures the port to listen on. Default: `8080`

#### streams

Contains a dictionary of different stream configurations.

### Stream configuration

#### path

Configures the path under which the proxied stream should be made available. In the example this value is `cam-1.mjpeg` for the stream `cam_1`. If the value does not start with  a `/` it is internally prepended. Mandatory

#### url

Configures the upstream url of the stream to proxy. In the example this value is `http://some-stream.com/stream.mjpeg` for `cam_1`. Mandatory

#### fps

Configures the output fps of the stream. Frames will be dropped or duplicated as needed. Default: 25


The example configures one server called `default` listening on `address` `127.0.0.1` on `port` `8080` serving two `streams` called `cam_1` and `cam_2`, available at `/cam-1.mjpeg` and `/cam-2.mjpeg` respectively, proxying `http://some-stream.com/stream.mjpeg` and `http://some-stream.com/stream2.mjpeg` respectively, both at `25` `fps`.

