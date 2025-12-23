if int(input()) % 2:
    def f(x, y):
        return x + y
else:
    def f(x,y):
        if x > 0:
            return y+f(x-1,y)
        return 0
print(f(10, 5))
