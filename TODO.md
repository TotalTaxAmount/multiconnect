# TODO

## All

- Write docs
- Write tests
- Better Error handling

## Protocol

- Refactor impls and util functions (traits?)
- Use 2 enums for everything (NetworkEvent and NetworkCommand)
- [Partial Done] Come up with a better way to send data p2p, ideally be able to keep a stream open between x amount of peers and send a receive data async

## Desktop

- Fix peer refresh (store a devices online status somewhere, ideally not in the saved peers json)
- Send results in a oneshot channel back to sender in backend context

## Android

- Android app

## Misc

- Rename `multiconnect-core` to `multiconnect-core`
