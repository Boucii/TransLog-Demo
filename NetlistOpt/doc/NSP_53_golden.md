| # | optimal | original_expr | number |
| ----- | ----------------------------------------------- | ----------------------------------------------- | ---- |
| 1 | (bridge a b d a c) | (+ (+ (+ (* a b) (* a c)) (* a d)) (* (* b c) d)) | 5 |
| 2 | (bridge a b d e c) | (+ (+ (+ (* a b) (* (* a c) e)) (* d e)) (* (* d b) c)) | 5 |
| 3 | (* e (bridge a b d a c)) | (+ (+ (+ (* (* e a) b) (* (* e a) c)) (* (* e a) d)) (* (* (* e b) c) d)) | 6 |
| 4 | (+ (bridge a b d a c) e) | (+ (+ (+ (+ e (* a b)) (* a c)) (* a d)) (* (* b c) d)) | 6 |
| 5 | (bridge a d b (* a e) c) | (+ (+ (+ (* (* a e) b) (* (* a e) c)) (* d a)) (* (* d b) c)) | 6 |
| 6 | (bridge a b d a (* c e)) | (+ (+ (+ (* a b) (* a d)) (* (* a c) e)) (* (* (* b d) c) e)) | 6 |
| 7 | (bridge a b c a (* d e)) | (+ (+ (+ (* a b) (* a c)) (* (* d e) a)) (* (* (* d e) b) c)) | 6 |
| 8 | (bridge a c b (+ a e) d) | (+ (+ (+ (+ (* a b) (* a c)) (* a d)) (* (* b c) d)) (* b e)) | 6 |
| 9 | (bridge a c b a (+ d e)) | (+ (+ (+ (+ (+ (* a b) (* a c)) (* a d)) (* a e)) (* (* b c) d)) (* (* b c) e)) | 6 |
| 10 | (bridge (* f a) b d e c) | (+ (+ (+ (* (* a f) b) (* (* (* a f) c) e)) (* (* d b) c)) (* d e)) | 6 |
| 11 | (bridge e d b (+ a f) c) | (+ (+ (+ (+ (+ (* a b) (* (* a c) e)) (* f b)) (* (* f c) e)) (* (* d b) c)) (* d e)) | 6 |
| 12 | (bridge a b d e (* c f)) | (+ (+ (+ (* (* (* c f) a) e) (* (* (* c f) b) d)) (* a b)) (* d e)) | 6 |
| 13 | (bridge a b d e (+ c f)) | (+ (+ (+ (+ (+ (* (* c a) e) (* (* c b) d)) (* (* f a) e)) (* (* f b) d)) (* a b)) (* d e)) | 6 |
| 14 | (* (* f e) (bridge a b c a d)) | (+ (+ (+ (* (* (* e f) a) b) (* (* (* e f) a) c)) (* (* (* e f) a) d)) (* (* (* (* e f) b) c) d)) | 7 |
| 15 | (* (bridge a b c a d) (+ f e)) | (+ (+ (+ (+ (+ (+ (+ (* (* e a) b) (* (* e a) c)) (* (* e a) d)) (* (* (* e b) c) d)) (* (* f a) b)) (* (* f a) c)) (* (* f a) d)) (* (* (* f b) c) d)) | 7 |
| 16 | (* (+ (bridge a b c a d) f) e) | (+ (+ (+ (+ (* e f) (* (* e a) b)) (* (* e a) c)) (* (* e a) d)) (* (* (* e b) c) d)) | 7 |
| 17 | (+ (bridge a b c a d) (* e f)) | (+ (+ (+ (+ (* e f) (* a b)) (* a c)) (* a d)) (* (* b c) d)) | 7 |
| 18 | (+ (+ (bridge a b c a d) f) e) | (+ (+ (+ (+ (+ e f) (* a b)) (* a c)) (* a d)) (* (* b c) d)) | 7 |
| 19 | (* f (bridge (* e a) b d a c)) | (+ (+ (+ (* (* (* f a) e) b) (* (* (* f a) e) c)) (* (* f d) a)) (* (* (* f d) b) c)) | 7 |
| 20 | (+ (bridge (* e a) b d a c) f) | (+ (+ (+ (+ f (* (* a e) b)) (* (* a e) c)) (* d a)) (* (* d b) c)) | 7 |
| 21 | (* f (bridge (* e c) a a b d)) | (+ (+ (+ (* (* (* f c) e) a) (* (* (* (* f c) e) b) d)) (* (* f a) b)) (* (* f a) d)) | 7 |
| 22 | (+ (bridge (* e c) a a b d) f) | (+ (+ (+ (+ f (* (* c e) a)) (* (* (* c e) b) d)) (* a b)) (* a d)) | 7 |
| 23 | (bridge (* (* a f) e) b d a c) | (+ (+ (+ (* (* (* a e) f) b) (* (* (* a e) f) c)) (* d a)) (* (* d b) c)) | 7 |
| 24 | (bridge a d b (+ (+ a e) f) c) | (+ (+ (+ (+ (+ (+ (+ (* d a) (* (* d b) c)) (* a b)) (* a c)) (* e b)) (* f b)) (* (* f c) a)) (* (* e c) a)) | 7 |
| 25 | (bridge (* (+ e f) a) b d a c) | (+ (+ (+ (+ (+ (* (* a e) b) (* (* a e) c)) (* (* a f) b)) (* (* a f) c)) (* d a)) (* (* d b) c)) | 7 |
| 26 | (bridge (+ (* e f) a) b c a d) | (+ (+ (+ (+ (+ (* (* d b) c) (* a b)) (* a c)) (* d a)) (* (* e f) b)) (* (* (* e f) c) a)) | 7 |
| 27 | (bridge (* (* f e) c) a a b d) | (+ (+ (+ (* (* (* c e) f) a) (* (* (* (* c e) f) b) d)) (* a b)) (* a d)) | 7 |
| 28 | (bridge b a a (+ (+ c e) f) d) | (+ (+ (+ (+ (+ (+ (+ (* c a) (* (* c b) d)) (* e a)) (* (* e b) d)) (* f a)) (* (* f b) d)) (* a b)) (* a d)) | 7 |
| 29 | (bridge (* (+ e f) c) a a b d) | (+ (+ (+ (+ (+ (* (* c e) a) (* (* (* c e) b) d)) (* (* c f) a)) (* (* (* c f) b) d)) (* a b)) (* a d)) | 7 |
| 30 | (bridge (+ (* e f) c) a a b d) | (+ (+ (+ (+ (+ (* c a) (* (* c b) d)) (* (* e f) a)) (* (* (* e f) b) d)) (* a b)) (* a d)) | 7 |
| 31 | (bridge (* e a) b d a (* c f)) | (+ (+ (+ (* (* a e) b) (* (* (* a e) c) f)) (* d a)) (* (* (* d b) c) f)) | 7 |
| 32 | (bridge (+ a e) b d a (* c f)) | (+ (+ (+ (+ (+ (* (* (* d b) c) f) (* a b)) (* d a)) (* (* a c) f)) (* e b)) (* (* (* e c) f) a)) | 7 |
| 33 | (bridge (* e a) b d a (+ f c)) | (+ (+ (+ (+ (+ (* (* a e) b) (* (* a e) c)) (* (* a e) f)) (* d a)) (* (* d b) c)) (* (* d b) f)) | 7 |
| 34 | (bridge (+ a e) (+ c f) b a d) | (+ (+ (+ (+ (+ (+ (+ (+ (* a b) (* a c)) (* a f)) (* (* e b) a)) (* e c)) (* e f)) (* d a)) (* (* d b) c)) (* (* d b) f)) | 7 |
| 35 | (bridge (* e a) b (* f d) a c) | (+ (+ (+ (* (* a e) b) (* (* a e) c)) (* (* d f) a)) (* (* (* d f) b) c)) | 7 |
| 36 | (bridge (+ a e) c b a (* d f)) | (+ (+ (+ (+ (+ (* a b) (* a c)) (* (* e b) a)) (* e c)) (* (* d f) a)) (* (* (* d f) b) c)) | 7 |
| 37 | (bridge (+ f d) a (* e a) b c) | (+ (+ (+ (+ (+ (* (* a e) b) (* (* a e) c)) (* d a)) (* (* d b) c)) (* f a)) (* (* f b) c)) | 7 |
| 38 | (bridge (+ a e) c b a (+ d f)) | (+ (+ (+ (+ (+ (+ (+ (* a b) (* a c)) (* (* e b) a)) (* e c)) (* d a)) (* (* d b) c)) (* f a)) (* (* f b) c)) | 7 |
| 39 | (bridge (* b f) (* a e) a d c) | (+ (+ (+ (* (* (* a e) b) f) (* (* a e) c)) (* d a)) (* (* (* d b) c) f)) | 7 |
| 40 | (bridge (+ a e) c (* b f) a d) | (+ (+ (+ (+ (+ (* (* a b) f) (* a c)) (* (* (* e b) f) a)) (* e c)) (* d a)) (* (* (* d b) c) f)) | 7 |
| 41 | (bridge (+ f b) (* a e) a d c) | (+ (+ (+ (+ (+ (* (* a e) b) (* (* a e) f)) (* (* a e) c)) (* d a)) (* (* d c) b)) (* (* d c) f)) | 7 |
| 42 | (bridge (+ a e) (+ b f) d a c) | (+ (+ (+ (+ (+ (+ (+ (+ (* a f) (* a b)) (* e f)) (* d a)) (* (* e c) a)) (* (* d c) f)) (* (* d c) b)) (* a c)) (* e b)) | 7 |
| 43 | (bridge b (* e a) (* a f) d c) | (+ (+ (+ (* (* a e) b) (* (* (* a e) c) f)) (* (* d a) f)) (* (* d b) c)) | 7 |
| 44 | (bridge (* (+ a e) f) c b a d) | (+ (+ (+ (+ (* a b) (* (* a c) f)) (* (* e c) f)) (* (* d a) f)) (* (* d b) c)) | 7 |
| 45 | (bridge (+ f a) d b (* e a) c) | (+ (+ (+ (+ (+ (* (* a e) b) (* (* a e) c)) (* (* (* a e) c) f)) (* d a)) (* d f)) (* (* d b) c)) | 7 |
| 46 | (bridge (+ a e) b d (+ f a) c) | (+ (+ (+ (+ (+ (+ (+ (* a d) (* a b)) (* a c)) (* (* b c) d)) (* b e)) (* d f)) (* (* c e) f)) (* (* a c) f)) | 7 |
| 47 | (bridge (* b e) a a c (* d f)) | (+ (+ (+ (* (* a b) e) (* a c)) (* (* d f) a)) (* (* (* (* d f) c) b) e)) | 7 |
| 48 | (bridge (+ b e) a a c (* d f)) | (+ (+ (+ (+ (+ (* a b) (* a e)) (* a c)) (* (* d f) a)) (* (* (* d f) c) b)) (* (* (* d f) c) e)) | 7 |
| 49 | (bridge (* b e) a a c (+ f d)) | (+ (+ (+ (+ (+ (* (* a b) e) (* a c)) (* d a)) (* (* (* d c) b) e)) (* f a)) (* (* (* f c) b) e)) | 7 |
| 50 | (bridge (+ b e) a a c (+ d f)) | (+ (+ (+ (+ (+ (+ (+ (+ (* a b) (* a e)) (* a c)) (* d a)) (* (* d c) b)) (* (* d c) e)) (* f a)) (* (* f c) b)) (* (* f c) e)) | 7 |
| 51 | (bridge a (* f c) (* b e) a d) | (+ (+ (+ (* (* a b) e) (* (* a c) f)) (* d a)) (* (* (* (* d b) c) e) f)) | 7 |
| 52 | (bridge (+ b e) a a (* f c) d) | (+ (+ (+ (+ (+ (* a b) (* a e)) (* (* a c) f)) (* d a)) (* (* (* d c) f) b)) (* (* (* d c) f) e)) | 7 |
| 53 | (bridge (+ b e) a a (+ c f) d) | (+ (+ (+ (+ (+ (+ (+ (+ (* a b) (* a e)) (* a c)) (* a f)) (* d a)) (* (* d c) b)) (* (* d c) e)) (* (* d f) b)) (* (* d f) e)) | 7 |
