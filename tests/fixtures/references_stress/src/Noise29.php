<?php

namespace App;

class Noise29
{
    private function compute(): int
    {
        return 29;
    }

    public function process(): int
    {
        return $this->compute() + $this->process();
    }
}
